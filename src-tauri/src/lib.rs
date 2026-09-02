mod alerts;
mod collector;
mod energy;
mod gguf;
mod local_ai;
mod throughput;
mod timed_command;

use collector::Collector;
use serde_json::{json, Value};
use std::collections::HashMap;
#[cfg(unix)]
use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;
use tauri::State;

#[derive(Clone)]
struct RuntimeState {
    latest: Arc<RwLock<Option<Value>>>,
    alert_settings: Arc<Mutex<HashMap<String, bool>>>,
    collector: Arc<Mutex<Collector>>,
}

/// Fresh look-up of a process at action time, independent of the (possibly
/// stale and truncated) UI snapshot. Returns the process name, or an error.
/// The protected check runs against this live data, closing the PID-recycling
/// window between "the UI showed this PID" and "the OS acts on it".
fn live_target(collector: &Mutex<Collector>, pid: u32) -> Result<Value, String> {
    let mut collector = collector
        .lock()
        .map_err(|_| "VISOR native collector is unavailable.".to_string())?;
    collector.refresh_processes_only();
    let system = collector.system_ref();
    let process = system
        .process(sysinfo::Pid::from_u32(pid))
        .ok_or_else(|| "The process is no longer running.".to_string())?;
    let raw_name = process.name().to_string_lossy().to_string();
    let name = if cfg!(windows)
        && !raw_name.contains('.')
        && !["System", "Registry", "Idle"].contains(&raw_name.as_str())
    {
        format!("{raw_name}.exe")
    } else {
        raw_name
    };
    let protected = collector::is_protected(pid, &name);
    Ok(json!({
        "id": pid,
        "name": name,
        "protected": protected,
        "path": process.exe().map(|value| value.to_string_lossy().to_string()).unwrap_or_default()
    }))
}

#[tauri::command]
fn get_system_snapshot(state: State<'_, RuntimeState>) -> Result<Value, String> {
    state
        .latest
        .read()
        .map_err(|_| "VISOR telemetry state is unavailable.".to_string())?
        .clone()
        .ok_or_else(|| "VISOR native telemetry is starting.".to_string())
}

#[tauri::command]
fn refresh_system_snapshot(state: State<'_, RuntimeState>) -> Result<Value, String> {
    let snapshot = collect_snapshot(&state, true)?;
    *state
        .latest
        .write()
        .map_err(|_| "VISOR telemetry state is unavailable.".to_string())? = Some(snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
fn kill_process(
    state: State<'_, RuntimeState>,
    pid: u32,
    confirmation: String,
    tree: bool,
    force: bool,
) -> Result<Value, String> {
    if confirmation != pid.to_string() {
        return Err("PID confirmation does not match.".to_string());
    }
    // Re-validate against a live process refresh, not the UI snapshot.
    let target = match live_target(&state.collector, pid) {
        Ok(target) => target,
        Err(_) => find_target(&state, pid)?,
    };
    if target
        .get("protected")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        return Err("VISOR protects this critical system process.".to_string());
    }
    #[cfg(windows)]
    {
        let mut args = vec!["/PID".to_string(), pid.to_string()];
        if tree {
            args.push("/T".to_string());
        }
        if force {
            args.push("/F".to_string());
        }
        let output = timed_command::output(
            hidden_command("taskkill.exe").args(args),
            None,
            Duration::from_secs(10),
        )
        .map_err(|error| error.to_string())?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if message.is_empty() {
                "Windows refused to terminate the process. Try running VISOR as administrator."
                    .to_string()
            } else {
                message
            });
        }
        Ok(json!({ "ok": true, "action": "kill", "pid": pid, "name": target.get("name") }))
    }
    #[cfg(all(not(windows), unix))]
    {
        if tree {
            let mut children = find_descendants(&state, pid)?;
            children.reverse();
            for child in children {
                let _ = timed_command::output(
                    hidden_command("kill")
                        .args([if force { "-KILL" } else { "-TERM" }, &child.to_string()]),
                    None,
                    Duration::from_secs(3),
                );
            }
        }
        let output = timed_command::output(
            hidden_command("kill").args([if force { "-KILL" } else { "-TERM" }, &pid.to_string()]),
            None,
            Duration::from_secs(5),
        )
        .map_err(|error| error.to_string())?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if message.is_empty() {
                "The operating system refused to terminate the process.".to_string()
            } else {
                message
            });
        }
        Ok(json!({ "ok": true, "action": "kill", "pid": pid, "name": target.get("name") }))
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (tree, force, target);
        Err("Process termination is not available on this platform.".to_string())
    }
}

#[tauri::command]
fn set_process_priority(
    state: State<'_, RuntimeState>,
    pid: u32,
    priority: String,
) -> Result<Value, String> {
    // Re-validate against a live process refresh, not the UI snapshot.
    let target = match live_target(&state.collector, pid) {
        Ok(target) => target,
        Err(_) => find_target(&state, pid)?,
    };
    if target
        .get("protected")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        return Err("VISOR protects this critical system process.".to_string());
    }
    let (windows_class, unix_nice) = match priority.as_str() {
        "low" => ("Idle", "10"),
        "belowNormal" => ("BelowNormal", "5"),
        "normal" => ("Normal", "0"),
        "aboveNormal" => ("AboveNormal", "-5"),
        "high" => ("High", "-10"),
        _ => return Err("Unsupported priority class.".to_string()),
    };
    #[cfg(windows)]
    {
        let _ = unix_nice;
        let script = "& { param([int]$TargetPid,[string]$Class) $p = Get-Process -Id $TargetPid -ErrorAction Stop; $p.PriorityClass = $Class }";
        let output = timed_command::output(
            hidden_command("powershell.exe").args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
                "-TargetPid",
                &pid.to_string(),
                "-Class",
                windows_class,
            ]),
            None,
            Duration::from_secs(10),
        )
        .map_err(|error| error.to_string())?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if message.is_empty() {
                "Windows refused the priority change. Try running VISOR as administrator."
                    .to_string()
            } else {
                message
            });
        }
        Ok(
            json!({ "ok": true, "action": "priority", "pid": pid, "name": target.get("name"), "priority": priority }),
        )
    }
    #[cfg(all(not(windows), unix))]
    {
        let _ = windows_class;
        let output = timed_command::output(
            hidden_command("renice").args([unix_nice, "-p", &pid.to_string()]),
            None,
            Duration::from_secs(5),
        )
        .map_err(|error| error.to_string())?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if message.is_empty() {
                "The operating system refused the priority change. Elevated permission may be required.".to_string()
            } else {
                message
            });
        }
        Ok(
            json!({ "ok": true, "action": "priority", "pid": pid, "name": target.get("name"), "priority": priority }),
        )
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (windows_class, unix_nice, target);
        Err("Priority control is not available on this platform.".to_string())
    }
}

#[tauri::command]
fn set_alert_rule(
    state: State<'_, RuntimeState>,
    id: String,
    enabled: bool,
) -> Result<Value, String> {
    let known = [
        "cpu-sustained",
        "gpu-sustained",
        "cpu-thermal",
        "gpu-thermal",
        "ssd-thermal",
        "vram-pressure",
    ];
    if !known.contains(&id.as_str()) {
        return Err("Unknown alert rule.".to_string());
    }
    state
        .alert_settings
        .lock()
        .map_err(|_| "VISOR alert settings are unavailable.".to_string())?
        .insert(id.clone(), enabled);
    Ok(json!({ "ok": true, "action": "alert-rule", "id": id, "enabled": enabled }))
}

fn find_target(state: &RuntimeState, pid: u32) -> Result<Value, String> {
    let guard = state
        .latest
        .read()
        .map_err(|_| "VISOR telemetry state is unavailable.".to_string())?;
    let snapshot = guard
        .as_ref()
        .ok_or_else(|| "VISOR telemetry is starting.".to_string())?;
    snapshot
        .get("processes")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("id").and_then(Value::as_u64) == Some(pid as u64))
                .cloned()
        })
        .ok_or_else(|| "The process is no longer running.".to_string())
}

#[cfg(unix)]
fn find_descendants(state: &RuntimeState, root_pid: u32) -> Result<Vec<u32>, String> {
    let guard = state
        .latest
        .read()
        .map_err(|_| "VISOR telemetry state is unavailable.".to_string())?;
    let processes = guard
        .as_ref()
        .and_then(|snapshot| snapshot.get("processes"))
        .and_then(Value::as_array)
        .ok_or_else(|| "VISOR process inventory is unavailable.".to_string())?;
    let mut descendants = Vec::new();
    let mut known = HashSet::from([root_pid]);
    loop {
        let mut added = false;
        for process in processes {
            let id = process
                .get("id")
                .and_then(Value::as_u64)
                .unwrap_or_default() as u32;
            let parent = process
                .get("parentId")
                .and_then(Value::as_u64)
                .unwrap_or_default() as u32;
            if id > 4 && known.contains(&parent) && known.insert(id) {
                descendants.push(id);
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    Ok(descendants)
}

fn start_collector(state: RuntimeState) {
    thread::Builder::new()
        .name("visor-collector".to_string())
        .spawn(move || loop {
            if let Ok(snapshot) = collect_snapshot(&state, false) {
                if let Ok(mut latest) = state.latest.write() {
                    *latest = Some(snapshot);
                }
            }
            thread::sleep(Duration::from_millis(1100));
        })
        .expect("failed to start VISOR collector");
}

fn collect_snapshot(state: &RuntimeState, force: bool) -> Result<Value, String> {
    let settings = state
        .alert_settings
        .lock()
        .map_err(|_| "VISOR alert settings are unavailable.".to_string())?
        .clone();
    let mut collector = state
        .collector
        .lock()
        .map_err(|_| "VISOR native collector is unavailable.".to_string())?;
    if force {
        collector.force_refresh();
    }
    for (id, enabled) in settings {
        collector.set_alert_rule(&id, enabled);
    }
    Ok(collector.collect())
}

fn hidden_command(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = RuntimeState {
        latest: Arc::new(RwLock::new(None)),
        alert_settings: Arc::new(Mutex::new(HashMap::new())),
        collector: Arc::new(Mutex::new(Collector::new())),
    };
    start_collector(state.clone());
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_system_snapshot,
            refresh_system_snapshot,
            kill_process,
            set_process_priority,
            set_alert_rule
        ])
        .run(tauri::generate_context!())
        .expect("error while running VISOR");
}
