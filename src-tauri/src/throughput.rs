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
/// The session file stores samples in the same shape `history_json` renders,
/// so a restart restores exactly what the charts expect.
pub fn export_samples() -> Vec<Value> {
    let empty = VecDeque::new();
    let history = history_store()
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or(empty);
    history
        .iter()
        .map(|sample| {
            json!({
                "decodeTps": (sample.decode_tps * 100.0).round() / 100.0,
                "prefillTps": (sample.prefill_tps * 100.0).round() / 100.0,
                "tokens": sample.tokens,
                "at": sample.epoch_ms,
                "evidence": sample.evidence
            })
        })
        .collect()
}

/// Restores samples written by a previous session. Unknown or malformed
/// entries are skipped rather than imported, and the ring keeps its capacity.
pub fn import_samples(samples: &[Value]) -> usize {
    let mut restored = 0;
    if let Ok(mut history) = history_store().lock() {
        history.clear();
        for sample in samples.iter().take(crate::session::MAX_SAMPLES) {
            let decode = sample.get("decodeTps").and_then(Value::as_f64);
            // Negative, non-finite rates or a missing timestamp cannot come
            // from a sample this module recorded; a hand-edited file must not
            // import them as measurements.
            let Some(decode_tps) = decode else {
                continue;
            };
            if !decode_tps.is_finite() || decode_tps < 0.0 {
                continue;
            }
            let Some(epoch_ms) = sample.get("at").and_then(Value::as_u64) else {
                continue;
            };
            let evidence = match sample.get("evidence").and_then(Value::as_str) {
                Some("metrics-endpoint") => "metrics-endpoint",
                _ => "runtime-log",
            };
            history.push_back(ThroughputSample {
                decode_tps,
                prefill_tps: sample
                    .get("prefillTps")
                    .and_then(Value::as_f64)
                    .unwrap_or_default(),
                tokens: sample
                    .get("tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                epoch_ms,
                evidence,
            });
            restored += 1;
        }
        while history.len() > HISTORY_CAPACITY {
            history.pop_front();
        }
    }
    restored
}

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

/// Sums a counter by exact name, tolerating only a runtime namespace prefix.
///
/// Substring matching looked tolerant right up until it was checked against real
/// runtime output. llama.cpp additionally publishes `prompt_tokens_cached_total`
/// (a subset of prompt tokens), average-duration gauges named
/// `prompt_tokens_seconds` / `predicted_tokens_seconds`, and
/// `spec_decode_*_tokens_total`; vLLM publishes both `prompt_tokens` and
/// `prompt_tokens_total` for the same quantity, plus per-request histograms.
/// Matching on fragments quietly summed several of those into one total and
/// inflated every rate, with nothing reported as failed.
fn counter_total(samples: &[(String, f64)], accepted: &[&str]) -> Option<f64> {
    let mut matched = false;
    let mut total = 0.0_f64;
    for (name, value) in samples {
        if accepted
            .iter()
            .any(|suffix| name == *suffix || name.ends_with(&format!(":{suffix}")))
        {
            matched = true;
            total += value.max(0.0);
        }
    }
    matched.then_some(total)
}

/// Accepted counter names per quantity, each without its runtime namespace.
/// Covers current llama.cpp (`tokens_predicted_*`), older llama.cpp
/// (`predicted_*`), and vLLM (`generation_tokens_total`, histogram `_sum`).
/// The `<counter>_created` unix-timestamp gauge Prometheus emits beside most
/// counters is excluded by construction, since it matches no name here.
const PROMPT_TOKENS: &[&str] = &["prompt_tokens_total"];
const DECODE_TOKENS: &[&str] = &[
    "tokens_predicted_total",
    "predicted_tokens_total",
    "generation_tokens_total",
];
const PROMPT_SECONDS: &[&str] = &[
    "prompt_seconds_total",
    "prompt_eval_seconds_total",
    "request_prefill_time_seconds_sum",
];
const DECODE_SECONDS: &[&str] = &[
    "tokens_predicted_seconds_total",
    "predicted_seconds_total",
    "request_decode_time_seconds_sum",
    "decode_seconds_total",
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
/// One completed-phase timing line from a runtime log.
#[derive(Clone, Copy, Debug)]
struct TimingLine {
    prompt: bool,
    tokens_per_second: f64,
    tokens: u64,
    /// Ollama labels each request `task <n>`. Without it a prefill from an
    /// earlier request could be published beside a newer decode, so the two
    /// numbers on screen would describe different runs.
    task: Option<u64>,
}

/// `slot print_timing: id 0 | task 398 | ...`
fn task_id(line: &str) -> Option<u64> {
    let digits: String = line
        .split_once("task ")?
        .1
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn parse_timing_line(line: &str) -> Option<TimingLine> {
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
    Some(TimingLine {
        prompt,
        tokens_per_second,
        tokens: tokens as u64,
        task: task_id(line),
    })
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

/// Stable hash identifying the exact request a reading came from, so the same
/// finished request is recognised across polls even while unrelated log lines
/// (HTTP access records) keep growing the file.
fn line_signature(identity: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in identity.as_bytes() {
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
    let mut decode: Option<TimingLine> = None;
    let mut prefill: Option<TimingLine> = None;
    for line in lines.into_iter().rev() {
        if decode.is_some() && prefill.is_some() {
            break;
        }
        let Some(timing) = parse_timing_line(line) else {
            continue;
        };
        if timing.prompt && prefill.is_none() {
            prefill = Some(timing);
        } else if !timing.prompt && decode.is_none() {
            decode = Some(timing);
        }
    }
    // A newer request that is still generating has published its prefill but
    // not its decode. Reporting that prefill beside the previous request's
    // decode would show two numbers from different runs as one result, so the
    // pair is only kept when the runtime's own task id says they belong
    // together; a decode-less prefill on its own stays reportable.
    let paired_prefill = match (&decode, &prefill) {
        (Some(current), Some(candidate)) if candidate.task == current.task => prefill,
        (Some(_), Some(_)) | (Some(_), None) => None,
        (None, _) => prefill,
    };
    let decode_tps = decode
        .map(|line| line.tokens_per_second)
        .unwrap_or_default();
    let decode_tokens = decode.map(|line| line.tokens).unwrap_or_default();
    let prefill_tps = paired_prefill
        .map(|line| line.tokens_per_second)
        .unwrap_or_default();
    if decode_tps <= 0.0 && prefill_tps <= 0.0 {
        return None;
    }
    let identity = format!(
        "d{:?}:{}:{}|p{:?}:{}",
        decode.map(|line| line.task),
        decode_tokens,
        decode_tps,
        paired_prefill.map(|line| line.task),
        prefill_tps
    );
    Some((
        decode_tps,
        prefill_tps,
        decode_tokens,
        line_signature(&identity),
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

    // Provenance for each file is recorded in its header. In short: the Ollama
    // log was captured live and cross-checked against that runtime's own API for
    // the same request; the two metrics fixtures carry authoritative names and
    // synthetic values, so they assert name handling and arithmetic, not realism.
    const OLLAMA_FIXTURE: &str = include_str!("../tests/fixtures/ollama-0.40.6-server.log");
    const LLAMACPP_FIXTURE: &str = include_str!("../tests/fixtures/llamacpp-metrics.txt");
    const VLLM_FIXTURE: &str = include_str!("../tests/fixtures/vllm-metrics.txt");

    /// The measurement cache and session history are process-wide so the
    /// collector can share them; tests must not inherit each other's state.
    fn reset_measurement_state() {
        if let Ok(mut guard) = CACHE.lock() {
            *guard = None;
        }
        if let Ok(mut history) = history_store().lock() {
            history.clear();
        }
    }

    #[test]
    fn parses_ollama_timing_lines() {
        let decode = parse_timing_line(
            "slot print_timing: id  0 | task 398 |        eval time =  471271.62 ms /   687 tokens (  686.98 ms per token,     1.46 tokens per second)",
        )
        .expect("decode line");
        assert!(!decode.prompt);
        assert!((decode.tokens_per_second - 1.46).abs() < 0.001);
        assert_eq!(decode.tokens, 687);
        assert_eq!(decode.task, Some(398));

        let prefill = parse_timing_line(
            "slot print_timing: id  0 | task 5528 | prompt eval time =    1725.09 ms /   119 tokens (   14.50 ms per token,    68.98 tokens per second)",
        )
        .expect("prefill line");
        assert!(prefill.prompt);
        assert!((prefill.tokens_per_second - 68.98).abs() < 0.001);
        assert_eq!(prefill.task, Some(5528));

        assert!(parse_timing_line("[GIN] 2026/09/02 - 14:26:47 | 200 | GET").is_none());
    }

    #[test]
    fn fixture_log_reports_the_cross_checked_request() {
        let (decode, prefill, tokens, _) = scan_log(OLLAMA_FIXTURE).expect("fixture timings");
        // Newest complete request in the fixture: task 1.
        assert!((decode - 3.57).abs() < 0.001);
        assert!((prefill - 5.97).abs() < 0.001);
        assert_eq!(tokens, 100);
    }

    #[test]
    fn prefill_from_a_newer_request_is_not_paired_with_an_older_decode() {
        // Task 1 is still generating: it has published its prefill but not its
        // eval line. Pairing it with task 0's decode would show two numbers from
        // different runs as one result, so only the decode is reported.
        let log = "\
slot print_timing: id  0 | task 0 | prompt eval time =  1000.00 ms /   100 tokens ( 10.00 ms per token,   100.00 tokens per second)
slot print_timing: id  0 | task 0 |        eval time = 20000.00 ms /   200 tokens (100.00 ms per token,    10.00 tokens per second)
slot print_timing: id  0 | task 1 | prompt eval time =   500.00 ms /   500 tokens (  1.00 ms per token,  1000.00 tokens per second)
";
        let (decode, prefill, tokens, _) = scan_log(log).expect("decode survives");
        assert!((decode - 10.0).abs() < 0.001);
        assert_eq!(prefill, 0.0);
        assert_eq!(tokens, 200);
    }

    #[test]
    fn llamacpp_counters_exclude_subset_and_average_series() {
        let samples = parse_prometheus(LLAMACPP_FIXTURE);
        // Both label series of prompt_tokens_total count (1438870 + 1000); the
        // 950000 cached subset and the 0.000144 average-duration gauge must not.
        assert_eq!(counter_total(&samples, PROMPT_TOKENS), Some(1_439_870.0));
        assert_eq!(counter_total(&samples, DECODE_TOKENS), Some(94_208.0));
        assert_eq!(counter_total(&samples, PROMPT_SECONDS), Some(207.21));
        assert_eq!(counter_total(&samples, DECODE_SECONDS), Some(1_540.5));
    }

    #[test]
    fn vllm_counters_do_not_sum_histogram_or_short_named_twins() {
        let samples = parse_prometheus(VLLM_FIXTURE);
        assert_eq!(counter_total(&samples, PROMPT_TOKENS), Some(250_000.0));
        assert_eq!(counter_total(&samples, DECODE_TOKENS), Some(180_000.0));
        assert_eq!(
            counter_total(&samples, PROMPT_SECONDS),
            Some(41.25),
            "only the prefill histogram _sum is a seconds total"
        );
        assert_eq!(
            counter_total(&samples, DECODE_SECONDS),
            Some(2_250.0),
            "decode histogram _sum, not _count or _bucket"
        );
    }

    #[test]
    fn prometheus_parser_skips_unusable_exposition() {
        let samples = parse_prometheus(
            "# HELP x\n# TYPE x counter\nbroken_line_without_value\nnan_metric{a=\"b\"} NaN\nonly_a_name 42\n",
        );
        assert_eq!(samples, vec![("only_a_name".to_string(), 42.0)]);
        assert!(parse_prometheus("# TYPE only\n\n").is_empty());
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
    fn export_import_round_trip_restores_the_same_samples() {
        reset_measurement_state();
        record_sample(ThroughputSample {
            decode_tps: 41.25,
            prefill_tps: 900.5,
            tokens: 64,
            epoch_ms: 1_700_000_000_000,
            evidence: "runtime-log",
        });
        let exported = export_samples();
        assert!(import_samples(&exported) >= 1);
        let rendered = history_json();
        let samples = rendered["samples"].as_array().expect("samples");
        let last = samples
            .iter()
            .rev()
            .find(|sample| sample["decodeTps"] == 41.25)
            .expect("restored");
        assert_eq!(last["at"], 1_700_000_000_000_u64);
        reset_measurement_state();
    }

    #[test]
    fn import_rejects_corrupt_entries() {
        reset_measurement_state();
        let poisoned = vec![
            json!({ "decodeTps": -5.0, "at": 1 }),
            json!({ "decodeTps": 3.0 }),
            json!({ "at": 2 }),
            json!({ "decodeTps": 7.5, "at": 3, "evidence": "runtime-log" }),
        ];
        assert_eq!(import_samples(&poisoned), 1);
        let rendered = history_json();
        assert_eq!(rendered["samples"].as_array().unwrap().len(), 1);
        assert_eq!(rendered["samples"][0]["decodeTps"], 7.5);
        reset_measurement_state();
    }

    #[test]
    fn created_and_short_named_twins_never_reach_a_total() {
        let samples = parse_prometheus(
            "vllm:prompt_tokens_total 100\nvllm:prompt_tokens 100\nvllm:generation_tokens_total 40\nvllm:generation_tokens 40\nvllm:prompt_tokens_created 5\n",
        );
        assert_eq!(counter_total(&samples, PROMPT_TOKENS), Some(100.0));
        assert_eq!(counter_total(&samples, DECODE_TOKENS), Some(40.0));
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

    // The log path can only be pointed at a real machine's Ollama log, so this
    // asserts the invariant that holds either way rather than a specific
    // provenance: an absent reading never carries an invented rate.
    #[test]
    fn unavailable_never_invents_a_rate() {
        reset_measurement_state();
        let throughput = measure_with(|| None);
        assert!(matches!(
            throughput.evidence,
            "unavailable" | "runtime-log" | "last-runtime-log" | "metrics-endpoint"
        ));
        if throughput.evidence == "unavailable" {
            assert!(throughput.decode_tokens_per_second.is_none());
            assert!(throughput.prefill_tokens_per_second.is_none());
        }
        // Whatever is reported, it must state when it was observed.
        if throughput.decode_tokens_per_second.is_some() {
            assert!(throughput.observed_at.is_some());
        }
    }

    #[test]
    fn history_rejects_a_repeated_completion_and_keeps_distinct_ones() {
        reset_measurement_state();
        let sample = ThroughputSample {
            decode_tps: 42.5,
            prefill_tps: 130.0,
            tokens: 128,
            epoch_ms: 1_000,
            evidence: "runtime-log",
        };
        record_sample(sample.clone());
        // Re-reading the same finished request on a later poll must not add a
        // second point, or the session totals count one run several times.
        assert!(
            !dedupe_recent(sample.decode_tps, sample.tokens),
            "identical consecutive completion must be rejected"
        );
        let faster = ThroughputSample {
            decode_tps: 44.0,
            ..sample.clone()
        };
        assert!(dedupe_recent(faster.decode_tps, faster.tokens));
        record_sample(faster);
        let rendered = history_json();
        let samples = rendered["samples"].as_array().expect("samples array");
        assert_eq!(samples.len(), 2, "expected exactly the two distinct runs");
        assert_eq!(samples[0]["decodeTps"], 42.5);
        assert_eq!(samples[1]["decodeTps"], 44.0);
        assert_eq!(samples[0]["evidence"], "runtime-log");
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
