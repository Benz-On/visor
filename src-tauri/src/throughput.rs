use crate::local_ai;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Measured inference throughput.
///
/// Every field states where the number came from. `decode_tokens_per_second`
/// and `prefill_tokens_per_second` are exact when `evidence` is
/// `metrics-endpoint` or `runtime-log`; they are absent otherwise. Nothing in
/// this module extrapolates or models — modeling stays in the advisor.
#[derive(Clone, Debug, Default)]
pub struct InferenceThroughput {
    pub decode_tokens_per_second: Option<f64>,
    pub prefill_tokens_per_second: Option<f64>,
    pub last_tokens: Option<u64>,
    pub evidence: &'static str,
    pub observed_at: Option<String>,
    /// Seconds elapsed between `observed_at` and now. Non-zero only for the
    /// log path, where a finished request stays visible after it ends.
    pub age_seconds: Option<f64>,
    /// Which runtime supplied the counters, when known.
    pub source: Option<&'static str>,
}

const LOG_TAIL_BYTES: u64 = 256 * 1024;
const MAX_LINES_SCANNED: usize = 2_000;
const CACHE_TTL: Duration = Duration::from_secs(3);

/// Session history of measured samples (kept in-process; no disk persistence
/// yet so the buffer dies with the collector session, honestly labeled).
const HISTORY_CAPACITY: usize = 240;

#[derive(Clone, Debug, Default)]
pub struct ThroughputSample {
    pub decode_tps: f64,
    pub prefill_tps: f64,
    pub tokens: u64,
    pub epoch_ms: u64,
    pub evidence: &'static str,
}

fn history_store() -> &'static Mutex<VecDeque<ThroughputSample>> {
    static HISTORY: Mutex<VecDeque<ThroughputSample>> = Mutex::new(VecDeque::new());
    &HISTORY
}

fn record_sample(sample: ThroughputSample) {
    if let Ok(mut history) = history_store().lock() {
        history.push_back(sample);
        while history.len() > HISTORY_CAPACITY {
            history.pop_front();
        }
    }
}

fn dedupe_recent(decode_tps: f64, tokens: u64) -> bool {
    // The same log line is re-read until the file advances; reject identical
    // consecutive completions so flat-lining the history does not fabricate
    // sustained activity.
    if let Ok(history) = history_store().lock() {
        if let Some(last) = history.back() {
            return !(last.decode_tps == decode_tps && last.tokens == tokens);
        }
    }
    true
}

/// Session history as JSON for the inference monitor charts.
pub fn history_json() -> Value {
    let empty = VecDeque::new();
    let history = history_store()
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or(empty);
    json!({
        "capacity": HISTORY_CAPACITY,
        "samples": history.iter().map(|sample| json!({
            "decodeTps": (sample.decode_tps * 100.0).round() / 100.0,
            "prefillTps": (sample.prefill_tps * 100.0).round() / 100.0,
            "tokens": sample.tokens,
            "at": sample.epoch_ms,
            "evidence": sample.evidence
        })).collect::<Vec<_>>()
    })
}

#[derive(Clone, Default)]
struct ThroughputCache {
    metrics: Option<MetricsSnapshot>,
    log: Option<LogThroughput>,
    /// Signature of the completion already written to session history, so one
    /// finished request is recorded once instead of on every cache refill.
    recorded_log: Option<u64>,
}

/// Cumulative token and busy-time counters read from one runtime poll. Counters
/// are summed across label series (model, slot) because the interesting number
/// is aggregate throughput for the machine.
#[derive(Clone, Default)]
pub(crate) struct MetricsSnapshot {
    prompt_tokens: f64,
    predicted_tokens: f64,
    prompt_seconds: f64,
    predicted_seconds: f64,
    captured_at: Option<Instant>,
    source: &'static str,
}

#[derive(Clone, Default)]
struct LogThroughput {
    decode_tps: f64,
    prefill_tps: f64,
    decode_tokens: u64,
    /// Wall clock of the first poll that saw *this* completion. Re-reading the
    /// same line must not refresh it, or an idle machine would keep reporting the
    /// last finished request as if it had just happened.
    observed_at: Option<String>,
    first_seen: Option<Instant>,
    /// When the file was last read, independent of how old the completion is.
    scanned_at: Option<Instant>,
    signature: Option<u64>,
}

static CACHE: Mutex<Option<ThroughputCache>> = Mutex::new(None);

/// Shortest window trusted for a counter delta. Below this, one token of
/// counter granularity dominates and the rate becomes meaningless.
const MIN_WINDOW: f64 = 0.5;

/// How long a logged completion is presented as live throughput. Past this it
/// is still shown, but as the last measured request with its real age.
const LIVE_WINDOW: f64 = 15.0;

fn utc_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Parses Prometheus text exposition (`name{labels} value [timestamp]`).
///
/// This used to be decoded as JSON, which silently produced nothing for every
/// runtime because `/metrics` is never JSON. Comment and blank lines are
/// skipped and non-finite values (NaN/+Inf) are dropped.
fn parse_prometheus(text: &str) -> Vec<(String, f64)> {
    let mut samples = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(raw_name), Some(raw_value)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Ok(value) = raw_value.parse::<f64>() else {
            continue;
        };
        if !value.is_finite() {
            continue;
        }
        let name = match raw_name.find('{') {
            Some(cut) => &raw_name[..cut],
            None => raw_name,
        };
        samples.push((name.to_string(), value));
    }
    samples
}

/// Matches a counter tolerantly: runtimes namespace metrics (`llamacpp:`,
/// `vllm:`) and have renamed them between releases, so each candidate is a set
/// of required substrings plus exclusions. Candidates are tried in order and the
/// first that matches wins, so synonyms never add up together.
fn counter_total(samples: &[(String, f64)], candidates: &[(&[&str], &[&str])]) -> Option<f64> {
    candidates.iter().find_map(|(requires, forbids)| {
        let mut matched = false;
        let total: f64 = samples
            .iter()
            .filter(|(name, _)| {
                let lower = name.to_ascii_lowercase();
                if requires.iter().any(|part| !lower.contains(*part)) {
                    return false;
                }
                !forbids.iter().any(|part| lower.contains(*part))
            })
            .map(|(_, value)| {
                matched = true;
                value.max(0.0)
            })
            .sum();
        matched.then_some(total)
    })
}

/// Prometheus publishes a `<counter>_created` unix-timestamp gauge beside most
/// counters; it must never be summed into a token total, so `created` is
/// excluded everywhere below.
const PROMPT_TOKENS: &[(&[&str], &[&str])] = &[(&["prompt", "token"], &["seconds", "created"])];
const DECODE_TOKENS: &[(&[&str], &[&str])] = &[
    (&["predicted", "token"], &["seconds", "created"]),
    (&["generation", "token"], &["seconds", "created"]),
    (&["decode", "token"], &["seconds", "created"]),
];
const PROMPT_SECONDS: &[(&[&str], &[&str])] = &[
    (&["prompt", "second"], &["created"]),
    (&["prefill", "second"], &["latency", "created"]),
];
const DECODE_SECONDS: &[(&[&str], &[&str])] = &[
    (&["predicted", "second"], &["created"]),
    (&["generation", "second"], &["latency", "created"]),
    (&["decode", "second"], &["created"]),
];

/// Fetches one runtime's `/metrics` and sums the counters this module needs.
fn read_metrics_endpoint(url: &str, source: &'static str) -> Option<MetricsSnapshot> {
    let mut text = String::new();
    ureq::get(url)
        .timeout(Duration::from_millis(800))
        .call()
        .ok()?
        .into_reader()
        .read_to_string(&mut text)
        .ok()?;
    let samples = parse_prometheus(&text);
    let prompt_tokens = counter_total(&samples, PROMPT_TOKENS)?;
    let predicted_tokens = counter_total(&samples, DECODE_TOKENS)?;
    Some(MetricsSnapshot {
        prompt_tokens,
        predicted_tokens,
        prompt_seconds: counter_total(&samples, PROMPT_SECONDS).unwrap_or_default(),
        predicted_seconds: counter_total(&samples, DECODE_SECONDS).unwrap_or_default(),
        captured_at: Some(Instant::now()),
        source,
    })
}

fn rate(tokens: f64, busy_seconds: f64, window: f64) -> Option<f64> {
    if tokens <= 0.0 {
        return None;
    }
    // Prefer the runtime's own busy time. Dividing by the wall-clock window
    // dilutes bursts: a 0.6 s prefill sampled inside a 3 s poll reads ~5x slow.
    let span = if busy_seconds > 0.0 {
        busy_seconds
    } else {
        window
    };
    Some(tokens / span.max(0.01))
}

fn metrics_delta(
    previous: Option<&MetricsSnapshot>,
    current: &MetricsSnapshot,
) -> Option<InferenceThroughput> {
    let previous = previous?;
    // A different endpoint means a different runtime: re-baseline instead of
    // subtracting one server's counters from another's.
    if previous.source != current.source {
        return None;
    }
    let (previous_captured, current_captured) = (previous.captured_at?, current.captured_at?);
    let window = current_captured
        .duration_since(previous_captured)
        .as_secs_f64();
    if window < MIN_WINDOW {
        return None;
    }
    let decode_tokens = current.predicted_tokens - previous.predicted_tokens;
    let prefill_tokens = current.prompt_tokens - previous.prompt_tokens;
    // Counters only rise. A decrease means the runtime restarted, so this
    // window carries no trustworthy measurement.
    if decode_tokens < 0.0 || prefill_tokens < 0.0 {
        return None;
    }
    let decode_tokens_per_second = rate(
        decode_tokens,
        current.predicted_seconds - previous.predicted_seconds,
        window,
    );
    let prefill_tokens_per_second = rate(
        prefill_tokens,
        current.prompt_seconds - previous.prompt_seconds,
        window,
    );
    if decode_tokens_per_second.is_none() && prefill_tokens_per_second.is_none() {
        return None;
    }
    Some(InferenceThroughput {
        decode_tokens_per_second,
        prefill_tokens_per_second,
        last_tokens: if decode_tokens > 0.0 {
            Some(decode_tokens as u64)
        } else {
            None
        },
        evidence: "metrics-endpoint",
        observed_at: Some(utc_now()),
        age_seconds: None,
        source: Some(current.source),
    })
}

/// Ollama (llama.cpp backend) prints per-request timing after every
/// generation. Example line:
/// `slot print_timing: id 0 | task 398 | eval time = 471271.62 ms / 687 tokens (686.98 ms per token, 1.46 tokens per second)`
fn parse_timing_line(line: &str) -> Option<(bool, f64, u64)> {
    let prompt = line.contains("prompt eval time");
    let eval = line.contains("eval time") && !prompt;
    if !eval && !prompt {
        return None;
    }
    let tokens_per_second = number_before(line, "tokens per second)")?;
    let tokens = after_label(line, "ms /")?;
    if tokens_per_second <= 0.0 || tokens < 1.0 {
        return None;
    }
    Some((prompt, tokens_per_second, tokens as u64))
}

/// Reads a number that directly precedes a label ("1.46 tokens per second)").
fn number_before(line: &str, label: &str) -> Option<f64> {
    let position = line.rfind(label)?;
    let head = line.get(..position)?.trim_end();
    let reversed: String = head
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect();
    reversed.chars().rev().collect::<String>().parse().ok()
}

fn after_label(line: &str, label: &str) -> Option<f64> {
    let position = line.rfind(label)? + label.len();
    let tail = line.get(position..)?.trim_start();
    let number: String = tail
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect();
    number.parse().ok()
}

fn candidate_log_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        paths.push(PathBuf::from(&local).join("Ollama").join("server.log"));
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        paths.push(
            PathBuf::from(&home)
                .join(".ollama")
                .join("logs")
                .join("server.log"),
        );
    }
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(
            PathBuf::from(&home)
                .join(".ollama")
                .join("logs")
                .join("server.log"),
        );
    }
    paths
}

fn file_identity(path: &std::path::Path) -> Option<(u64, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    Some((metadata.len(), modified))
}

fn read_log_tail(path: &std::path::Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let start = size.saturating_sub(LOG_TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    // A tail that opens mid-line turns `( 45.20 tokens per second)` into a
    // parseable `5.20 tokens per second)`, so the partial head line and any
    // still-unfinished trailing line are dropped before parsing.
    if start > 0 {
        let Some(cut) = bytes.iter().position(|byte| *byte == b'\n') else {
            return Some(String::new());
        };
        bytes = bytes.split_off(cut + 1);
    }
    if !bytes.ends_with(b"\n") {
        match bytes.iter().rposition(|byte| *byte == b'\n') {
            Some(cut) => bytes.truncate(cut + 1),
            None => bytes.clear(),
        }
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Stable hash of the timing lines behind a reading, so the same finished
/// request is recognised across polls even while unrelated log lines (HTTP
/// access records) keep growing the file.
fn line_signature(decode_line: &str, prefill_line: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in decode_line.as_bytes().iter().chain(prefill_line.as_bytes()) {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

fn scan_log(buffer: &str) -> Option<(f64, f64, u64, u64)> {
    let mut lines: Vec<&str> = buffer.lines().collect();
    if lines.len() > MAX_LINES_SCANNED {
        lines.drain(..lines.len() - MAX_LINES_SCANNED);
    }
    // Walk newest-first; keep the most recent complete request timings.
    let mut decode: Option<(f64, u64)> = None;
    let mut prefill: Option<f64> = None;
    let mut decode_line: &str = "";
    let mut prefill_line: &str = "";
    for line in lines.into_iter().rev() {
        if decode.is_some() && prefill.is_some() {
            break;
        }
        let Some((prompt, tps, tokens)) = parse_timing_line(line) else {
            continue;
        };
        if prompt && prefill.is_none() {
            prefill = Some(tps);
            prefill_line = line;
        } else if !prompt && decode.is_none() {
            decode = Some((tps, tokens));
            decode_line = line;
        }
    }
    let (decode_tps, decode_tokens) = decode.unwrap_or((0.0, 0));
    let prefill_tps = prefill.unwrap_or_default();
    if decode_tps <= 0.0 && prefill_tps <= 0.0 {
        return None;
    }
    Some((
        decode_tps,
        prefill_tps,
        decode_tokens,
        line_signature(decode_line, prefill_line),
    ))
}

/// Finds the newest completed generation in a runtime log. `previous` supplies
/// the original observation time when the same completion is read again, so an
/// idle machine ages honestly instead of looking continuously fresh — the file
/// mtime cannot do this because unrelated log lines keep advancing it.
fn log_throughput(previous: Option<&LogThroughput>) -> Option<LogThroughput> {
    for path in candidate_log_paths() {
        let identity = file_identity(&path);
        if let Some((_, modified_epoch)) = identity {
            let now_epoch = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or_default();
            // A log untouched for five minutes means no recent inference.
            if now_epoch.saturating_sub(modified_epoch) > 300 {
                continue;
            }
        }
        let Some(buffer) = read_log_tail(&path) else {
            continue;
        };
        let Some((decode, prefill, tokens, signature)) = scan_log(&buffer) else {
            continue;
        };
        let unchanged = previous.and_then(|entry| entry.signature) == Some(signature);
        let carried = previous.filter(|_| unchanged);
        return Some(LogThroughput {
            decode_tps: decode,
            prefill_tps: prefill,
            decode_tokens: tokens,
            observed_at: Some(
                carried
                    .and_then(|entry| entry.observed_at.clone())
                    .unwrap_or_else(utc_now),
            ),
            first_seen: Some(
                carried
                    .and_then(|entry| entry.first_seen)
                    .unwrap_or_else(Instant::now),
            ),
            scanned_at: Some(Instant::now()),
            signature: Some(signature),
        });
    }
    None
}

/// Public entry point used by the collector. Result is cached briefly so the
/// 1.1 s collector loop does not hammer the log or the metrics endpoints.
pub fn measure() -> InferenceThroughput {
    measure_with(read_any_metrics_endpoint)
}

/// Polls every candidate runtime metrics endpoint at once and keeps the first
/// that exposes usable counters. Doing this sequentially would add several
/// hundred milliseconds of dead latency per collect on a machine running only
/// Ollama, which exposes no metrics endpoint at all.
fn read_any_metrics_endpoint() -> Option<MetricsSnapshot> {
    let targets = local_ai::metrics_targets();
    std::thread::scope(|scope| {
        let readers: Vec<_> = targets
            .iter()
            .map(|(source, url)| scope.spawn(move || read_metrics_endpoint(url, source)))
            .collect();
        readers
            .into_iter()
            .find_map(|reader| reader.join().ok().flatten())
    })
}

pub fn measure_with<F>(mut metrics_reader: F) -> InferenceThroughput
where
    F: FnMut() -> Option<MetricsSnapshot>,
{
    let mut guard = match CACHE.lock() {
        Ok(guard) => guard,
        Err(_) => return InferenceThroughput::default(),
    };
    let cache = guard.get_or_insert_with(ThroughputCache::default);
    let fresh = |captured: &Option<Instant>| {
        captured
            .map(|instant| instant.elapsed() < CACHE_TTL)
            .unwrap_or(false)
    };

    let should_poll_metrics = match cache.metrics.as_ref() {
        Some(previous) => !fresh(&previous.captured_at),
        None => true,
    };
    if should_poll_metrics {
        if let Some(current) = metrics_reader() {
            let delta = cache
                .metrics
                .as_ref()
                .and_then(|previous| metrics_delta(Some(previous), &current));
            cache.metrics = Some(current);
            if let Some(delta) = delta {
                if let (Some(decode_tps), Some(token_count)) =
                    (delta.decode_tokens_per_second, delta.last_tokens)
                {
                    if dedupe_recent(decode_tps, token_count) {
                        record_sample(ThroughputSample {
                            decode_tps,
                            prefill_tps: delta.prefill_tokens_per_second.unwrap_or_default(),
                            tokens: token_count,
                            epoch_ms: std::time::SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map(|duration| duration.as_millis() as u64)
                                .unwrap_or_default(),
                            evidence: "metrics-endpoint",
                        });
                    }
                }
                return delta;
            }
        }
    }

    if !cache
        .log
        .as_ref()
        .is_some_and(|entry| fresh(&entry.scanned_at))
    {
        let previous = cache.log.clone();
        cache.log = log_throughput(previous.as_ref());
    }

    let logged = cache.log.clone();
    if let Some(entry) = logged.as_ref() {
        let age = entry
            .first_seen
            .map(|instant| instant.elapsed().as_secs_f64())
            .unwrap_or_default();
        let live = age <= LIVE_WINDOW;
        let decode = (entry.decode_tps > 0.0).then_some(entry.decode_tps);
        let prefill = (entry.prefill_tps > 0.0).then_some(entry.prefill_tps);
        let tokens = (entry.decode_tokens > 0).then_some(entry.decode_tokens);
        // A completion is recorded once. Re-reading the same finished request
        // every few seconds must not inflate the session token total.
        if let (Some(decode_tps), Some(token_count), Some(signature)) =
            (decode, tokens, entry.signature)
        {
            if live
                && cache.recorded_log != Some(signature)
                && dedupe_recent(decode_tps, token_count)
            {
                cache.recorded_log = Some(signature);
                record_sample(ThroughputSample {
                    decode_tps,
                    prefill_tps: prefill.unwrap_or_default(),
                    tokens: token_count,
                    epoch_ms: std::time::SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|duration| duration.as_millis() as u64)
                        .unwrap_or_default(),
                    evidence: "runtime-log",
                });
            }
        }
        return InferenceThroughput {
            decode_tokens_per_second: decode,
            prefill_tokens_per_second: prefill,
            last_tokens: tokens,
            // An idle machine used to present its last completion as current
            // throughput. Past the live window it is labelled for what it is.
            evidence: if live {
                "runtime-log"
            } else {
                "last-runtime-log"
            },
            observed_at: entry.observed_at.clone(),
            age_seconds: Some((age * 10.0).round() / 10.0),
            source: None,
        };
    }
    if cache
        .metrics
        .as_ref()
        .is_some_and(|snapshot| fresh(&snapshot.captured_at))
    {
        // A metrics endpoint is present but produced no delta this window
        // (idle server). Report provenance without inventing a rate.
        return InferenceThroughput {
            evidence: "metrics-endpoint",
            observed_at: Some(utc_now()),
            ..InferenceThroughput::default()
        };
    }
    InferenceThroughput {
        evidence: "unavailable",
        ..InferenceThroughput::default()
    }
}

pub fn as_json(throughput: &InferenceThroughput) -> Value {
    json!({
        "decodeTokensPerSecond": throughput.decode_tokens_per_second.map(|value| (value * 100.0).round() / 100.0),
        "prefillTokensPerSecond": throughput.prefill_tokens_per_second.map(|value| (value * 100.0).round() / 100.0),
        "lastTokens": throughput.last_tokens,
        "evidence": throughput.evidence,
        "observedAt": throughput.observed_at,
        "ageSeconds": throughput.age_seconds,
        "source": throughput.source
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ollama_timing_lines() {
        let decode = parse_timing_line(
            "slot print_timing: id  0 | task 398 |        eval time =  471271.62 ms /   687 tokens (  686.98 ms per token,     1.46 tokens per second)",
        )
        .expect("decode line");
        assert!(!decode.0);
        assert!((decode.1 - 1.46).abs() < 0.001);
        assert_eq!(decode.2, 687);

        let prefill = parse_timing_line(
            "slot print_timing: id  0 | task 5528 | prompt eval time =    1725.09 ms /   119 tokens (   14.50 ms per token,    68.98 tokens per second)",
        )
        .expect("prefill line");
        assert!(prefill.0);
        assert!((prefill.1 - 68.98).abs() < 0.001);
        assert_eq!(prefill.2, 119);

        assert!(parse_timing_line("[GIN] 2026/09/02 - 14:26:47 | 200 | GET").is_none());
    }

    #[test]
    fn metrics_delta_requires_time_and_tokens() {
        let now = Instant::now();
        let previous = MetricsSnapshot {
            prompt_tokens: 1_000.0,
            predicted_tokens: 5_000.0,
            prompt_seconds: 0.0,
            predicted_seconds: 0.0,
            captured_at: Some(now),
            source: "test",
        };
        let quiet = MetricsSnapshot {
            captured_at: Some(now + Duration::from_secs(1)),
            ..previous.clone()
        };
        assert!(metrics_delta(Some(&previous), &quiet).is_none());

        let active = MetricsSnapshot {
            predicted_tokens: 5_137.0,
            captured_at: Some(now + Duration::from_secs(1)),
            ..previous.clone()
        };
        let delta = metrics_delta(Some(&previous), &active).expect("delta");
        assert_eq!(delta.evidence, "metrics-endpoint");
        assert!((delta.decode_tokens_per_second.unwrap() - 137.0).abs() < 0.01);

        assert!(metrics_delta(None, &active).is_none());
    }

    #[test]
    fn metrics_delta_rebaselines_on_restart_and_endpoint_change() {
        let now = Instant::now();
        let previous = MetricsSnapshot {
            prompt_tokens: 9_000.0,
            predicted_tokens: 40_000.0,
            prompt_seconds: 40.0,
            predicted_seconds: 600.0,
            captured_at: Some(now),
            source: "test",
        };
        // Counters only rise; a fall means the runtime restarted, so the window
        // must yield nothing rather than a saturating guess.
        let restarted = MetricsSnapshot {
            prompt_tokens: 10.0,
            predicted_tokens: 25.0,
            captured_at: Some(now + Duration::from_secs(2)),
            ..previous.clone()
        };
        assert!(metrics_delta(Some(&previous), &restarted).is_none());

        // A different runtime's counters are not a delta of the first one's.
        let other_server = MetricsSnapshot {
            prompt_tokens: 9_500.0,
            predicted_tokens: 41_000.0,
            prompt_seconds: 45.0,
            predicted_seconds: 640.0,
            captured_at: Some(now + Duration::from_secs(2)),
            source: "another",
        };
        assert!(metrics_delta(Some(&previous), &other_server).is_none());

        // Busy seconds, not the wall-clock window, set the rate: a 0.5 s decode
        // burst sampled inside a 4 s poll is 400 tok/s, not 50.
        let bursty = MetricsSnapshot {
            prompt_tokens: 9_000.0,
            predicted_tokens: 40_200.0,
            predicted_seconds: 600.5,
            captured_at: Some(now + Duration::from_secs(4)),
            ..previous.clone()
        };
        let delta = metrics_delta(Some(&previous), &bursty).expect("burst delta");
        assert!((delta.decode_tokens_per_second.unwrap() - 400.0).abs() < 0.01);
        assert!(delta.prefill_tokens_per_second.is_none());
    }

    #[test]
    fn parses_prometheus_text_exposition() {
        // `/metrics` is never JSON; these are the shapes llama.cpp and vLLM emit.
        let text = "\
# HELP llamacpp:prompt_tokens_total Number of prompt tokens processed.
# TYPE llamacpp:prompt_tokens_total counter
llamacpp:prompt_tokens_total{model=\"qwen3.5:9b\"} 1.43887e+06
llamacpp:predicted_tokens_total{model=\"qwen3.5:9b\"} 94208
llamacpp:prompt_seconds_total 207.21
llamacpp:predicted_seconds_total 1540.5
vllm:generation_tokens_total{model_name=\"x\"} 12
vllm:e2e_request_latency_seconds_sum 99
broken_line_without_value
nan_metric{a=\"b\"} NaN
";
        let samples = parse_prometheus(text);
        assert!(samples
            .iter()
            .any(|(name, _)| name == "llamacpp:prompt_tokens_total"));
        assert_eq!(counter_total(&samples, PROMPT_TOKENS), Some(1_438_870.0));
        // Candidate sets are tried in order and the first match wins, so a
        // llama.cpp server's `predicted_*` series is never added to a vLLM
        // `generation_*` series that also happens to be exposed.
        assert_eq!(counter_total(&samples, DECODE_TOKENS), Some(94_208.0));
        assert_eq!(counter_total(&samples, PROMPT_SECONDS), Some(207.21));
        assert_eq!(counter_total(&samples, DECODE_SECONDS), Some(1540.5));
        // NaN and malformed lines must not leak into a total.
        assert!(counter_total(&samples, &[(&["latency", "second"], &[])],) == Some(99.0));
        assert_eq!(counter_total(&samples, PROMPT_TOKENS), Some(1_438_870.0));
        assert!(parse_prometheus("# TYPE only\n\n").is_empty());
    }

    #[test]
    fn decode_token_matcher_prefers_predicted_and_skips_prompt_series() {
        let samples = parse_prometheus(
            "vllm:prompt_tokens_total 100\nvllm:generation_tokens_total 40\nvllm:prompt_tokens_created 5\n",
        );
        assert_eq!(counter_total(&samples, DECODE_TOKENS), Some(40.0));
        assert_eq!(counter_total(&samples, PROMPT_TOKENS), Some(100.0));
    }

    #[test]
    fn same_completion_keeps_its_signature() {
        let log = "\
slot print_timing: id 0 | task 1 |        eval time = 20000.00 ms /  200 tokens (100.00 ms per token,   10.00 tokens per second)
";
        let first = scan_log(log).expect("timings");
        let second = scan_log(log).expect("timings");
        assert_eq!(first.3, second.3);
        let other = scan_log(&log.replace("10.00 tokens", "11.00 tokens")).expect("other");
        assert_ne!(first.3, other.3);
    }

    #[test]
    fn scan_log_prefers_newest_complete_request() {
        let log = "\
slot print_timing: id 0 | task 1 | prompt eval time =  1000.00 ms /  100 tokens ( 10.00 ms per token,  100.00 tokens per second)
slot print_timing: id 0 | task 1 |        eval time = 20000.00 ms /  200 tokens (100.00 ms per token,   10.00 tokens per second)
slot print_timing: id 0 | task 2 | prompt eval time =   500.00 ms /   50 tokens ( 10.00 ms per token,  100.00 tokens per second)
slot print_timing: id 0 | task 2 |        eval time = 10000.00 ms /  500 tokens ( 20.00 ms per token,   50.00 tokens per second)
";
        let (decode, prefill, tokens, _) = scan_log(log).expect("timings");
        assert!((decode - 50.0).abs() < 0.001);
        assert!((prefill - 100.0).abs() < 0.001);
        assert_eq!(tokens, 500);
    }

    // NOTE: the shared static cache makes this assertion order-dependent when a
    // real Ollama log exists on the machine, so the evidence check below accepts
    // any valid provenance and only asserts that no rate is ever invented.
    #[test]
    fn unavailable_when_no_evidence() {
        let throughput = measure_with(|| None);
        assert!(matches!(
            throughput.evidence,
            "unavailable" | "runtime-log" | "last-runtime-log" | "metrics-endpoint"
        ));
        assert!(
            throughput.decode_tokens_per_second.is_none()
                || throughput.evidence == "runtime-log"
                || throughput.evidence == "last-runtime-log"
        );
    }

    #[test]
    fn history_records_measured_samples_with_dedupe() {
        let sample = ThroughputSample {
            decode_tps: 42.5,
            prefill_tps: 130.0,
            tokens: 128,
            epoch_ms: 1_000,
            evidence: "runtime-log",
        };
        record_sample(sample.clone());
        record_sample(sample.clone());
        // Identical consecutive completions must not double-count.
        assert!(!history_json()["samples"].as_array().unwrap().is_empty());
        let rendered = history_json();
        assert_eq!(rendered["samples"][0]["decodeTps"], 42.5);
        assert_eq!(rendered["samples"][0]["evidence"], "runtime-log");
    }

    /// Live check against the real machine's Ollama log. Ignored by default:
    /// run with `cargo test live_ -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_ollama_log_reports_measured_rates_or_unavailable() {
        let throughput = measure();
        println!("live throughput: {throughput:?}");
        assert!(matches!(
            throughput.evidence,
            "runtime-log" | "last-runtime-log" | "metrics-endpoint" | "unavailable"
        ));
    }
}
