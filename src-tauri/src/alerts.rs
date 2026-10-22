use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

const RECENT_LIMIT: usize = 6;

#[derive(Clone)]
struct Rule {
    id: &'static str,
    title: &'static str,
    metric: &'static str,
    threshold: f64,
    duration: Duration,
    unit: &'static str,
    suppress_during_gaming: bool,
    severity: &'static str,
}

/// Sustained-condition evaluation with resolution history.
///
/// `recent` records every sustained alert that naturally returned below its
/// threshold: the peak seen while it was active, when it started, and when it
/// resolved. Disabling a rule clears its state without writing history, so a
/// switched-off rule never invents a resolution it was never allowed to see.
pub struct AlertEngine {
    started: HashMap<&'static str, Instant>,
    enabled: HashMap<String, bool>,
    /// Highest value observed during the current exceedance.
    peaks: HashMap<&'static str, f64>,
    /// Wall-clock time a rule first became fully active.
    activated_at: HashMap<&'static str, String>,
    /// Wall-clock time a rule first exceeded, stamped before the sustain
    /// window elapsed — this is the `startedAt` history records report.
    first_exceeded_at: HashMap<&'static str, String>,
    /// Rules that were active on the previous evaluation.
    was_active: HashSet<&'static str>,
    recent: VecDeque<Value>,
}

impl AlertEngine {
    pub fn new() -> Self {
        Self {
            started: HashMap::new(),
            enabled: HashMap::new(),
            peaks: HashMap::new(),
            activated_at: HashMap::new(),
            first_exceeded_at: HashMap::new(),
            was_active: HashSet::new(),
            recent: VecDeque::new(),
        }
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if !rules().iter().any(|rule| rule.id == id) {
            return false;
        }
        self.enabled.insert(id.to_string(), enabled);
        if !enabled {
            // An explicit disable is a user action, not a resolution: the rule's
            // running state is dropped without being recorded as history.
            self.started.remove(id);
            self.peaks.remove(id);
            self.activated_at.remove(id);
            self.first_exceeded_at.remove(id);
            self.was_active.remove(id);
        }
        true
    }

    pub fn evaluate(&mut self, metrics: &Value, gaming: Option<(String, u32)>) -> Value {
        let now = Instant::now();
        let mut active = Vec::new();
        let mut watch = Vec::new();
        let mut rendered_rules = Vec::new();

        for rule in rules() {
            let enabled = *self.enabled.get(rule.id).unwrap_or(&true);
            let value = metrics
                .get(rule.metric)
                .and_then(Value::as_f64)
                .unwrap_or_default();
            let sensor_available = !rule.metric.ends_with("Temp") || value > 0.0;
            let suppressed = enabled && rule.suppress_during_gaming && gaming.is_some();
            let exceeded = enabled && sensor_available && value >= rule.threshold && !suppressed;
            if exceeded {
                if !self.started.contains_key(&rule.id) {
                    self.first_exceeded_at
                        .entry(rule.id)
                        .or_insert_with(|| chrono::Utc::now().to_rfc3339());
                }
                self.started.entry(rule.id).or_insert(now);
                let peak = self.peaks.entry(rule.id).or_insert(0.0);
                if value > *peak {
                    *peak = value;
                }
            } else {
                self.started.remove(rule.id);
            }
            let elapsed = self
                .started
                .get(rule.id)
                .map(|started| now.duration_since(*started))
                .unwrap_or_default();
            let is_active = exceeded && elapsed >= rule.duration;
            if is_active {
                self.activated_at
                    .entry(rule.id)
                    .or_insert_with(|| chrono::Utc::now().to_rfc3339());
            }
            // Only a genuine return below threshold writes history. Suppression
            // starting (a game launched) or a sensor going away ends the
            // episode without inventing a resolution the UI would then claim
            // happened because "the condition returned below its threshold".
            let naturally_cleared = sensor_available && value < rule.threshold && !suppressed;
            if self.was_active.contains(&rule.id) && !exceeded {
                if naturally_cleared {
                    self.record_resolution(&rule);
                } else {
                    self.drop_episode(rule.id);
                }
            } else if !exceeded {
                // A watch-level excursion that faded must not leave its peak
                // behind for a later real episode to inherit.
                self.peaks.remove(rule.id);
                self.activated_at.remove(rule.id);
                self.first_exceeded_at.remove(rule.id);
            }
            if is_active {
                self.was_active.insert(rule.id);
            } else {
                self.was_active.remove(rule.id);
            }
            let progress = if exceeded {
                ((elapsed.as_secs_f64() / rule.duration.as_secs_f64()) * 100.0)
                    .round()
                    .min(100.0) as u64
            } else {
                0
            };
            let since = self.activated_at.get(rule.id).cloned();
            let rule_json = json!({
                "id": rule.id,
                "title": rule.title,
                "metric": rule.metric,
                "threshold": rule.threshold,
                "durationMs": rule.duration.as_millis(),
                "unit": rule.unit,
                "suppressDuringGaming": rule.suppress_during_gaming,
                "enabled": enabled,
                "severity": rule.severity
            });
            let state = json!({
                "id": rule.id,
                "title": rule.title,
                "metric": rule.metric,
                "threshold": rule.threshold,
                "durationMs": rule.duration.as_millis(),
                "unit": rule.unit,
                "suppressDuringGaming": rule.suppress_during_gaming,
                "enabled": enabled,
                "severity": rule.severity,
                "value": value,
                "sensorAvailable": sensor_available,
                "suppressed": suppressed,
                "elapsedMs": elapsed.as_millis(),
                "progress": progress,
                "active": is_active,
                "since": since
            });
            rendered_rules.push(rule_json);
            if is_active {
                active.push(state);
            } else if exceeded || suppressed {
                watch.push(state);
            }
        }

        let gaming_active = gaming.is_some();
        let (process_name, pid) = gaming
            .map(|item| (Some(item.0), Some(item.1)))
            .unwrap_or((None, None));
        json!({
            "active": active,
            "watch": watch,
            "recent": self.recent.iter().cloned().collect::<Vec<_>>(),
            "gaming": {
                "active": gaming_active,
                "processName": process_name,
                "pid": pid,
                "suppressedRuleIds": if gaming_active { vec!["cpu-sustained", "gpu-sustained"] } else { Vec::<&str>::new() }
            },
            "rules": rendered_rules
        })
    }

    /// Writes one history record and drops the rule's running state. The
    /// `startedAt` is when the condition first exceeded, not when it became
    /// sustained, so the record spans the whole episode.
    fn record_resolution(&mut self, rule: &Rule) {
        let started_at = self
            .first_exceeded_at
            .remove(rule.id)
            .or_else(|| self.activated_at.remove(rule.id));
        if let (Some(peak), Some(started_at)) = (self.peaks.remove(rule.id), started_at) {
            self.recent.push_front(json!({
                "id": format!("{}:{}", rule.id, chrono::Utc::now().timestamp_millis()),
                "ruleId": rule.id,
                "title": rule.title,
                "peak": (peak * 10.0).round() / 10.0,
                "unit": rule.unit,
                "startedAt": started_at,
                "resolvedAt": chrono::Utc::now().to_rfc3339(),
                "gamingSuppressed": false,
            }));
            while self.recent.len() > RECENT_LIMIT {
                self.recent.pop_back();
            }
        }
        self.started.remove(rule.id);
        self.activated_at.remove(rule.id);
        self.was_active.remove(rule.id);
    }

    /// Ends an episode without writing history: suppression started, or the
    /// sensor vanished while the rule was active.
    fn drop_episode(&mut self, id: &'static str) {
        self.started.remove(id);
        self.peaks.remove(id);
        self.activated_at.remove(id);
        self.first_exceeded_at.remove(id);
        self.was_active.remove(id);
    }
}

fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "cpu-sustained",
            title: "CPU sustained load",
            metric: "cpu",
            threshold: 90.0,
            duration: Duration::from_secs(120),
            unit: "%",
            suppress_during_gaming: true,
            severity: "warning",
        },
        Rule {
            id: "gpu-sustained",
            title: "GPU sustained load",
            metric: "gpu",
            threshold: 95.0,
            duration: Duration::from_secs(90),
            unit: "%",
            suppress_during_gaming: true,
            severity: "warning",
        },
        Rule {
            id: "cpu-thermal",
            title: "CPU temperature",
            metric: "cpuTemp",
            threshold: 90.0,
            duration: Duration::from_secs(30),
            unit: "°C",
            suppress_during_gaming: false,
            severity: "critical",
        },
        Rule {
            id: "gpu-thermal",
            title: "GPU temperature",
            metric: "gpuTemp",
            threshold: 86.0,
            duration: Duration::from_secs(30),
            unit: "°C",
            suppress_during_gaming: false,
            severity: "critical",
        },
        Rule {
            id: "ssd-thermal",
            title: "SSD temperature",
            metric: "ssdTemp",
            threshold: 70.0,
            duration: Duration::from_secs(60),
            unit: "°C",
            suppress_during_gaming: false,
            severity: "warning",
        },
        Rule {
            id: "vram-pressure",
            title: "VRAM pressure",
            metric: "vram",
            threshold: 95.0,
            duration: Duration::from_secs(30),
            unit: "%",
            suppress_during_gaming: false,
            severity: "warning",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An engine with one rule forced into the "sustained and active" state,
    /// as if the condition had held for longer than the rule's duration.
    fn engine_with_active_rule(id: &'static str, peak: f64) -> AlertEngine {
        let mut engine = AlertEngine::new();
        engine
            .started
            .insert(id, Instant::now() - Duration::from_secs(600));
        engine.peaks.insert(id, peak);
        engine
            .activated_at
            .insert(id, chrono::Utc::now().to_rfc3339());
        engine.was_active.insert(id);
        engine
    }

    #[test]
    fn sustained_alert_that_clears_is_recorded_with_peak_and_times() {
        let mut engine = engine_with_active_rule("cpu-sustained", 97.4);
        let cleared = engine.evaluate(&json!({ "cpu": 40.0 }), None);
        let recent = cleared["recent"].as_array().expect("history recorded");
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0]["ruleId"], "cpu-sustained");
        assert_eq!(recent[0]["peak"], 97.4);
        assert!(recent[0]["startedAt"].is_string());
        assert!(recent[0]["resolvedAt"].is_string());
        assert_eq!(recent[0]["gamingSuppressed"], false);
        // The rule's running state is gone, so a second clear adds nothing.
        let again = engine.evaluate(&json!({ "cpu": 10.0 }), None);
        assert_eq!(again["recent"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn disabling_a_rule_does_not_write_history() {
        let mut engine = engine_with_active_rule("gpu-sustained", 99.0);
        assert!(engine.set_enabled("gpu-sustained", false));
        let snapshot = engine.evaluate(&json!({ "gpu": 99.0 }), None);
        assert_eq!(snapshot["recent"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn watch_level_spikes_never_become_history() {
        let mut engine = AlertEngine::new();
        // Above threshold, but nowhere near the sustained duration.
        let watched = engine.evaluate(&json!({ "cpu": 95.0 }), None);
        assert!(watched["watch"].as_array().unwrap().len() == 1);
        let cleared = engine.evaluate(&json!({ "cpu": 20.0 }), None);
        assert_eq!(cleared["recent"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn suppression_ending_an_episode_writes_no_history() {
        let mut engine = engine_with_active_rule("cpu-sustained", 96.0);
        // A game launches: suppression ends the episode without a resolution.
        let suppressed =
            engine.evaluate(&json!({ "cpu": 96.0 }), Some(("game.exe".to_string(), 42)));
        assert_eq!(suppressed["recent"].as_array().unwrap().len(), 0);
        // And the episode's state is gone, so the same overload afterwards
        // starts a fresh sustain window rather than inheriting the old one.
        let still_watching = engine.evaluate(&json!({ "cpu": 96.0 }), None);
        assert!(still_watching["watch"].as_array().unwrap().len() == 1);
        assert_eq!(still_watching["recent"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn sensor_loss_ending_an_episode_writes_no_history() {
        let mut engine = engine_with_active_rule("cpu-thermal", 95.0);
        // cpuTemp absent -> value 0 -> sensor unavailable. The episode ends
        // without the UI later claiming the temperature "returned below its
        // threshold" when it merely stopped being readable.
        let degraded = engine.evaluate(&json!({}), None);
        assert_eq!(degraded["recent"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn faded_watch_excursion_does_not_leak_its_peak() {
        let mut engine = AlertEngine::new();
        // A short excursion to 97 that never sustains, then clears.
        engine.evaluate(&json!({ "cpu": 97.0 }), None);
        engine.evaluate(&json!({ "cpu": 20.0 }), None);
        // A real sustained episode peaking lower must report its own peak.
        engine
            .started
            .insert("cpu-sustained", Instant::now() - Duration::from_secs(600));
        engine.peaks.insert("cpu-sustained", 92.0);
        engine
            .activated_at
            .insert("cpu-sustained", chrono::Utc::now().to_rfc3339());
        engine
            .first_exceeded_at
            .insert("cpu-sustained", chrono::Utc::now().to_rfc3339());
        engine.was_active.insert("cpu-sustained");
        let cleared = engine.evaluate(&json!({ "cpu": 10.0 }), None);
        let recent = cleared["recent"].as_array().expect("recorded");
        assert_eq!(recent[0]["peak"], 92.0);
    }

    #[test]
    fn history_keeps_at_most_six_resolutions() {
        let mut engine = AlertEngine::new();
        for index in 0..8 {
            engine
                .started
                .insert("cpu-sustained", Instant::now() - Duration::from_secs(600));
            engine.peaks.insert("cpu-sustained", 90.0 + index as f64);
            engine
                .activated_at
                .insert("cpu-sustained", chrono::Utc::now().to_rfc3339());
            engine.was_active.insert("cpu-sustained");
            engine.evaluate(&json!({ "cpu": 10.0 }), None);
        }
        let snapshot = engine.evaluate(&json!({ "cpu": 10.0 }), None);
        assert_eq!(snapshot["recent"].as_array().unwrap().len(), 6);
    }
}
