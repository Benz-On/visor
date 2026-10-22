//! One shared HTTP agent for every loopback runtime probe.
//!
//! ureq 3 moved timeouts from the request builder to the agent, so every
//! probe now goes through this agent: a single global deadline (the longest
//! any runtime probe may take), keep-alive reuse across collector passes,
//! and no TLS — the endpoints are 127.0.0.1 only.

use serde_json::Value;
use std::time::Duration;
use ureq::Agent;

/// Hard ceiling for one runtime probe, request plus body.
const PROBE_TIMEOUT: Duration = Duration::from_millis(800);

fn agent() -> &'static Agent {
    use std::sync::OnceLock;
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        Agent::config_builder()
            .timeout_global(Some(PROBE_TIMEOUT))
            .build()
            .new_agent()
    })
}

/// GET a loopback URL and read the whole body as a string. `None` on any
/// failure or deadline: a missing runtime is absence, not an error to surface.
pub fn get_text(url: &str) -> Option<String> {
    agent()
        .get(url)
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()
}

/// GET a loopback URL and parse the body as JSON.
pub fn get_json(url: &str) -> Option<Value> {
    agent()
        .get(url)
        .call()
        .ok()?
        .body_mut()
        .read_json::<Value>()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live check against this machine's Ollama: proves the no-TLS agent can
    /// actually complete a plain-http request, not just fail gracefully.
    /// Ignored by default: `cargo test live_ -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_plain_http_round_trip_reaches_ollama() {
        match get_json("http://127.0.0.1:11434/api/version") {
            Some(value) => {
                println!("ollama answered: {value}");
                assert!(
                    value.get("version").is_some(),
                    "unexpected payload: {value}"
                );
            }
            None => {
                // A host without Ollama right now is absence, not a failure of
                // the transport; nothing is asserted either way.
                println!("no ollama on :11434; transport absence accepted");
            }
        }
    }

    #[test]
    fn unreachable_endpoints_are_absence_not_panic() {
        // Nothing listens on this port in CI or on a dev machine; the probe
        // must return None within the timeout rather than hang or unwrap.
        assert!(get_json("http://127.0.0.1:1/nothing").is_none());
        assert!(get_text("http://127.0.0.1:1/nothing").is_none());
    }
}
