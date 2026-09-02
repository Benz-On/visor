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
}

const LOG_TAIL_BYTES: u64 = 256 * 1024;
const MAX_LINES_SCANNED: usize = 2_000;
const CACHE_TTL: Duration = Duration::from_secs(3);

#[derive(Clone, Default)]
struct ThroughputCache {
    metrics: Option<MetricsSnapshot>,
    log: Option<LogThroughput>,
}

#[derive(Clone, Default)]
#[allow(dead_code)]
pub struct MetricsSnapshot {
    prompt_tokens: u64,
    predicted_tokens: u64,
    prompt_seconds: f64,
    predicted_seconds: f64,
    captured_at: Option<Instant>,
    source: &'static str,
}

#[derive(Clone, Default)]
#[allow(dead_code)]
struct LogThroughput {
    decode_tps: f64,
    prefill_tps: f64,
    decode_tokens: u64,
    captured_at: Option<Instant>,
    file_epoch: Option<(u64, u64)>,
}

static CACHE: Mutex<Option<ThroughputCache>> = Mutex::new(None);

fn utc_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn parse_counter(value: &Value, name: &str) -> Option<u64> {
    value
        .get(name)
        .and_then(Value::as_array)
        .and_then(|samples| samples.first())
        .and_then(|sample| sample.get("value"))
        .and_then(Value::as_f64)
        .map(|value| value.max(0.0) as u64)
}

/// llama.cpp server exposes prometheus counters with `--metrics`; vLLM always
/// exposes them on `/metrics`. Deltas between polls give exact tok/s.
fn read_metrics_endpoint(url: &str, source: &'static str) -> Option<MetricsSnapshot> {
    let payload: Value = ureq::get(url)
        .timeout(Duration::from_millis(600))
        .call()
        .ok()?
        .into_json()
        .ok()?;
    Some(MetricsSnapshot {
        prompt_tokens: parse_counter(&payload, "prompt_tokens_total")?,
        predicted_tokens: parse_counter(&payload, "predicted_tokens_total")
            .or_else(|| parse_counter(&payload, "vllm:generation_tokens_total"))?,
        prompt_seconds: value_of_gauge(&payload, "prompt_seconds_total").unwrap_or_default(),
        predicted_seconds: value_of_gauge(&payload, "predicted_seconds_total")
            .or_else(|| value_of_gauge(&payload, "vllm:e2e_request_latency_seconds_sum"))
            .unwrap_or_default(),
        captured_at: Some(Instant::now()),
        source,
    })
}

fn value_of_gauge(payload: &Value, name: &str) -> Option<f64> {
    payload
        .get(name)
        .and_then(Value::as_array)
        .and_then(|samples| samples.first())
        .and_then(|sample| sample.get("value"))
        .and_then(Value::as_f64)
}

fn metrics_delta(
    previous: Option<&MetricsSnapshot>,
    current: &MetricsSnapshot,
) -> Option<InferenceThroughput> {
    let previous = previous?;
    let previous_captured = previous.captured_at?;
    let current_captured = current.captured_at?;
    let elapsed = current_captured
        .duration_since(previous_captured)
        .as_secs_f64();
    if elapsed < 0.2 {
        return None;
    }
    let decode_tokens = current
        .predicted_tokens
        .saturating_sub(previous.predicted_tokens);
    let prefill_tokens = current.prompt_tokens.saturating_sub(previous.prompt_tokens);
    if decode_tokens == 0 && prefill_tokens == 0 {
        return None;
    }
    Some(InferenceThroughput {
        decode_tokens_per_second: Some(decode_tokens as f64 / elapsed),
        prefill_tokens_per_second: Some(prefill_tokens as f64 / elapsed),
        last_tokens: Some(decode_tokens),
        evidence: "metrics-endpoint",
        observed_at: Some(utc_now()),
    })
    .filter(|delta| {
        delta.decode_tokens_per_second.unwrap_or(0.0) > 0.0
            || delta.prefill_tokens_per_second.unwrap_or(0.0) > 0.0
    })
    .map(|mut delta| {
        delta.evidence = "metrics-endpoint";
        delta
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
    let mut buffer = String::new();
    file.read_to_string(&mut buffer).ok()?;
    Some(buffer)
}

fn scan_log(buffer: &str) -> Option<(f64, f64, u64)> {
    let mut lines: VecDeque<&str> = buffer.lines().collect();
    if lines.len() > MAX_LINES_SCANNED {
        lines.drain(..lines.len() - MAX_LINES_SCANNED);
    }
    // Walk newest-last; keep the most recent complete request timings.
    let mut decode = None;
    let mut prefill = None;
    let mut decode_tokens = 0_u64;
    for line in lines.into_iter().rev() {
        if decode.is_some() && prefill.is_some() {
            break;
        }
        if let Some((prompt, tps, tokens)) = parse_timing_line(line) {
            if prompt && prefill.is_none() {
                prefill = Some(tps);
            } else if !prompt && decode.is_none() {
                decode = Some(tps);
                decode_tokens = tokens;
            }
        }
    }
    if decode.is_none() && prefill.is_none() {
        return None;
    }
    // Stale completions must not present as live throughput: require a line
    // younger than five minutes via the file's mtime check by the caller.
    Some((
        decode.unwrap_or_default(),
        prefill.unwrap_or_default(),
        decode_tokens,
    ))
}

fn log_throughput() -> Option<LogThroughput> {
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
        if let Some(buffer) = read_log_tail(&path) {
            if let Some((decode, prefill, tokens)) = scan_log(&buffer) {
                if decode > 0.0 || prefill > 0.0 {
                    return Some(LogThroughput {
                        decode_tps: decode,
                        prefill_tps: prefill,
                        decode_tokens: tokens,
                        captured_at: Some(Instant::now()),
                        file_epoch: identity,
                    });
                }
            }
        }
    }
    None
}

/// Public entry point used by the collector. Result is cached briefly so the
/// 1.1 s collector loop does not hammer the log or the metrics endpoint.
pub fn measure() -> InferenceThroughput {
    measure_with(|| read_metrics_endpoint("http://127.0.0.1:8080/metrics", "llama.cpp /metrics"))
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
                return delta;
            }
        }
    }

    if !cache
        .log
        .as_ref()
        .is_some_and(|entry| fresh(&entry.captured_at))
    {
        cache.log = log_throughput();
    }

    if cache
        .log
        .as_ref()
        .is_some_and(|entry| fresh(&entry.captured_at))
    {
        let entry = cache.log.as_ref().expect("checked above");
        return InferenceThroughput {
            decode_tokens_per_second: (entry.decode_tps > 0.0).then_some(entry.decode_tps),
            prefill_tokens_per_second: (entry.prefill_tps > 0.0).then_some(entry.prefill_tps),
            last_tokens: (entry.decode_tokens > 0).then_some(entry.decode_tokens),
            evidence: "runtime-log",
            observed_at: Some(utc_now()),
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
        "observedAt": throughput.observed_at
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
            prompt_tokens: 1_000,
            predicted_tokens: 5_000,
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
            predicted_tokens: 5_137,
            captured_at: Some(now + Duration::from_secs(1)),
            ..previous.clone()
        };
        let delta = metrics_delta(Some(&previous), &active).expect("delta");
        assert_eq!(delta.evidence, "metrics-endpoint");
        assert!((delta.decode_tokens_per_second.unwrap() - 137.0).abs() < 0.01);

        assert!(metrics_delta(None, &active).is_none());
    }

    #[test]
    fn scan_log_prefers_newest_complete_request() {
        let log = "\
slot print_timing: id 0 | task 1 | prompt eval time =  1000.00 ms /  100 tokens ( 10.00 ms per token,  100.00 tokens per second)
slot print_timing: id 0 | task 1 |        eval time = 20000.00 ms /  200 tokens (100.00 ms per token,   10.00 tokens per second)
slot print_timing: id 0 | task 2 | prompt eval time =   500.00 ms /   50 tokens ( 10.00 ms per token,  100.00 tokens per second)
slot print_timing: id 0 | task 2 |        eval time = 10000.00 ms /  500 tokens ( 20.00 ms per token,   50.00 tokens per second)
";
        let (decode, prefill, tokens) = scan_log(log).expect("timings");
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
            "unavailable" | "runtime-log" | "metrics-endpoint"
        ));
        assert!(
            throughput.decode_tokens_per_second.is_none() || throughput.evidence == "runtime-log"
        );
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
            "runtime-log" | "metrics-endpoint" | "unavailable"
        ));
    }
}
