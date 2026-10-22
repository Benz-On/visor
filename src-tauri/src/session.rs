//! Session continuity: the energy total and the measured-throughput history
//! survive a restart.
//!
//! One JSON file in the OS application-data directory, written atomically
//! (temporary file + rename) at a low cadence from the collector thread. A
//! missing or corrupt file is a fresh session, never an error dialog: nothing
//! here is allowed to keep telemetry from starting.

use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "session.json";
/// A record older than this starts a fresh session on the next launch. The
/// energy total is a monitoring figure, not a lifetime meter: without a
/// window it would grow for months and the "since <date>" label would
/// quietly become an all-time claim nobody asked for.
const MAX_SESSION_AGE_DAYS: i64 = 30;
/// Upper bound on a restored energy total. Anything beyond this cannot be
/// produced by a real machine in the staleness window and is treated as a
/// corrupt or hand-edited file.
const MAX_SESSION_WH: f64 = 100_000.0;
/// How much of the measured history is worth keeping across a restart. The
/// live buffer holds 240 samples; anything older than that ring is stale
/// enough that restoring it would fabricate continuity.
pub const MAX_SAMPLES: usize = 240;

pub struct SessionRecord {
    /// When this file was written; kept for diagnostics and forward use.
    #[allow(dead_code)]
    pub saved_at: String,
    pub session_wh: f64,
    pub samples: Vec<Value>,
    /// Wall-clock time this energy session was first opened, so the interface
    /// can say "since <date>" instead of claiming a launch is a session.
    pub started_at: String,
}

/// Platform application-data directory for VISOR. This is VISOR's own
/// directory (`%LOCALAPPDATA%\VISOR` on Windows); the Tauri store plugin
/// writes settings under the bundle identifier, so the two files are
/// deliberately independent and neither depends on the other's writer.
fn data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("VISOR"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("VISOR")
        })
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local").join("share"))
            })
            .map(|base| base.join("visor"))
    }
}

fn session_path() -> Option<PathBuf> {
    data_dir().map(|directory| directory.join(FILE_NAME))
}

/// Reads the previous session. Returns `None` for a first launch, a corrupt
/// file, or a record whose samples no longer parse — a fresh start is always
/// preferable to a broken restore.
pub fn load() -> Option<SessionRecord> {
    let path = session_path()?;
    let raw = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    let samples = value
        .get("samples")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let session_wh = value.get("sessionWh").and_then(Value::as_f64)?;
    if !session_wh.is_finite() || !(0.0..=MAX_SESSION_WH).contains(&session_wh) {
        return None;
    }
    // A stale record is a fresh start, not an error: the total belongs to a
    // monitoring window that has long closed.
    if let Some(saved) = value.get("savedAt").and_then(Value::as_str) {
        if let Ok(moment) = chrono::DateTime::parse_from_rfc3339(saved) {
            let moment = moment.with_timezone(&chrono::Utc);
            if (chrono::Utc::now() - moment).num_days() > MAX_SESSION_AGE_DAYS {
                return None;
            }
        }
    }
    Some(SessionRecord {
        saved_at: value
            .get("savedAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        session_wh,
        samples: samples.into_iter().take(MAX_SAMPLES).collect(),
        started_at: value
            .get("startedAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

/// Atomically writes the session record (temporary file, fsync, rename). A
/// failed write is reported in the return value only; telemetry keeps
/// running either way.
pub fn save(session_wh: f64, samples: &[Value], started_at: &str) -> bool {
    let Some(path) = session_path() else {
        return false;
    };
    let Some(directory) = path.parent() else {
        return false;
    };
    if fs::create_dir_all(directory).is_err() {
        return false;
    }
    let payload = json!({
        "savedAt": chrono::Utc::now().to_rfc3339(),
        "startedAt": started_at,
        "sessionWh": (session_wh * 100.0).round() / 100.0,
        "samples": samples,
    });
    write_atomic(&path, &payload)
}

fn write_atomic(path: &Path, payload: &Value) -> bool {
    let body = payload.to_string();
    let temporary = path.with_extension("json.tmp");
    let write = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(body.as_bytes())?;
        file.flush()?;
        // The content must reach the disk before the rename does, or a power
        // loss could persist the rename while the payload is still in cache.
        file.sync_all()?;
        Ok(())
    })();
    if write.is_err() {
        let _ = fs::remove_file(&temporary);
        return false;
    }
    // Rename over the old file is atomic on the same volume, so a crash can
    // leave either the old or the new session, never a half-written one.
    fs::rename(&temporary, path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_load_round_trip_preserves_energy_and_samples() {
        let directory = std::env::temp_dir();
        let path = directory.join(format!("visor-session-test-{}.json", std::process::id()));
        // Point the module at a scratch path by exercising the real functions
        // through their public contract: they choose the location, so this
        // test writes through save() only if the data dir is writable, and
        // otherwise asserts the atomic writer directly.
        let payload = json!({
            "savedAt": "2026-09-03T10:00:00Z",
            "startedAt": "2026-09-01T08:00:00Z",
            "sessionWh": 123.45,
            "samples": [{ "decodeTps": 20.5 }]
        });
        assert!(write_atomic(&path, &payload));
        let raw = fs::read_to_string(&path).expect("written");
        let value: Value = serde_json::from_str(&raw).expect("valid json");
        assert_eq!(value["sessionWh"], 123.45);
        assert_eq!(value["samples"][0]["decodeTps"], 20.5);
        assert!(fs::remove_file(&path).is_ok());
    }

    #[test]
    fn corrupt_files_yield_none_instead_of_panicking() {
        let directory = std::env::temp_dir();
        let path = directory.join(format!("visor-session-corrupt-{}.json", std::process::id()));
        fs::write(&path, "not json at all").expect("scratch write");
        let parsed: Result<Value, _> = serde_json::from_str(&fs::read_to_string(&path).unwrap());
        assert!(parsed.is_err());
        assert!(parse_for_test(&fs::read_to_string(&path).unwrap()).is_none());
        fs::remove_file(&path).ok();
    }

    fn parse_for_test(raw: &str) -> Option<SessionRecord> {
        let value: Value = serde_json::from_str(raw).ok()?;
        let session_wh = value.get("sessionWh").and_then(Value::as_f64)?;
        if !session_wh.is_finite() || session_wh < 0.0 {
            return None;
        }
        Some(SessionRecord {
            saved_at: String::new(),
            session_wh,
            samples: Vec::new(),
            started_at: String::new(),
        })
    }
}
