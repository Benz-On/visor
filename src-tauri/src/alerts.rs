use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};

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

pub struct AlertEngine {
    started: HashMap<&'static str, Instant>,
    enabled: HashMap<String, bool>,
}

impl AlertEngine {
    pub fn new() -> Self {
        Self {
            started: HashMap::new(),
            enabled: HashMap::new(),
        }
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if !rules().iter().any(|rule| rule.id == id) {
            return false;
        }
        self.enabled.insert(id.to_string(), enabled);
        if !enabled {
            self.started.remove(id);
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
                self.started.entry(rule.id).or_insert(now);
            } else {
                self.started.remove(rule.id);
            }
            let elapsed = self
                .started
                .get(rule.id)
                .map(|started| now.duration_since(*started))
                .unwrap_or_default();
            let is_active = exceeded && elapsed >= rule.duration;
            let progress = if exceeded {
                ((elapsed.as_secs_f64() / rule.duration.as_secs_f64()) * 100.0)
                    .round()
                    .min(100.0) as u64
            } else {
                0
            };
            let since = self
                .started
                .get(rule.id)
                .map(|_| chrono::Utc::now().to_rfc3339());
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
            "recent": [],
            "gaming": {
                "active": gaming_active,
                "processName": process_name,
                "pid": pid,
                "suppressedRuleIds": if gaming_active { vec!["cpu-sustained", "gpu-sustained"] } else { Vec::<&str>::new() }
            },
            "rules": rendered_rules
        })
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
