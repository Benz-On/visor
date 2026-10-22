mod alerts;
mod collector;
mod energy;
mod gguf;
mod http;
mod local_ai;
mod session;
mod shell;
mod throughput;
mod timed_command;

use collector::Collector;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::collections::HashSet;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_window_state::StateFlags;

#[derive(Clone)]
struct RuntimeState {
    latest: Arc<RwLock<Option<Value>>>,
    alert_settings: Arc<Mutex<HashMap<String, bool>>>,
    collector: Arc<Mutex<Collector>>,
    /// Shell switches the window cannot own, because the tray and the menu have
    /// to read them while the webview is hidden.
    shell: Arc<ShellFlags>,
}

#[derive(Debug, Default)]
struct ShellFlags {
    paused: AtomicBool,
    notifications: AtomicBool,
    close_to_tray: AtomicBool,
}

impl ShellFlags {
    fn snapshot(&self) -> Value {
        json!({
            "paused": self.paused.load(Ordering::Relaxed),
            "notifications": self.notifications.load(Ordering::Relaxed),
            "closeToTray": self.close_to_tray.load(Ordering::Relaxed),
        })
    }
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
    // Having no sample yet is a warm-up state, not a failure. Returning an error
    // here made the interface report the collector as unavailable during the
    // several seconds its first pass legitimately takes, so a missing snapshot
    // comes back as null and the caller decides what that looks like.
    Ok(state
        .latest
        .read()
        .map_err(|_| "VISOR telemetry state is unavailable.".to_string())?
        .clone()
        .unwrap_or(Value::Null))
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

fn start_collector(state: RuntimeState, app: AppHandle) {
    thread::Builder::new()
        .name("visor-collector".to_string())
        .spawn(move || {
            let mut notified: HashSet<String> = HashSet::new();
            loop {
                if let Ok(snapshot) = collect_snapshot(&state, false) {
                    // The tray and the notification path read the snapshot before it
                    // is published, so they stay correct while the window is hidden
                    // and nobody is polling.
                    let tooltip =
                        shell::tooltip(&snapshot, state.shell.paused.load(Ordering::Relaxed));
                    shell::update_tray(&app, &tooltip);
                    if state.shell.notifications.load(Ordering::Relaxed) {
                        for (title, body) in shell::fresh_alerts(&snapshot, &mut notified) {
                            shell::notify(&app, &title, &body);
                        }
                    } else {
                        notified.clear();
                    }
                    if let Ok(mut latest) = state.latest.write() {
                        *latest = Some(snapshot);
                    }
                }
                thread::sleep(Duration::from_millis(1100));
            }
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
    // The window-hiding flag exists only on Windows, so the binding needs `mut`
    // there and nowhere else. The allow keeps every platform building under
    // `-D warnings` without splitting the function per target.
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

#[tauri::command]
fn get_throughput_history() -> Result<Value, String> {
    Ok(throughput::history_json())
}

#[tauri::command]
fn get_shell_state(state: State<'_, RuntimeState>) -> Value {
    state.shell.snapshot()
}

#[tauri::command]
fn set_shell_flags(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    paused: Option<bool>,
    notifications: Option<bool>,
    close_to_tray: Option<bool>,
) -> Value {
    if let Some(value) = paused {
        state.shell.paused.store(value, Ordering::Relaxed);
    }
    if let Some(value) = notifications {
        state.shell.notifications.store(value, Ordering::Relaxed);
    }
    if let Some(value) = close_to_tray {
        state.shell.close_to_tray.store(value, Ordering::Relaxed);
    }
    // The tray repeats the paused state, so refresh it now rather than letting the
    // next sample contradict the switch the user just moved.
    let latest = state.latest.read().ok().and_then(|guard| guard.clone());
    let tooltip = shell::tooltip(
        latest.as_ref().unwrap_or(&Value::Null),
        state.shell.paused.load(Ordering::Relaxed),
    );
    shell::update_tray(&app, &tooltip);
    state.shell.snapshot()
}

#[tauri::command]
fn set_price_factors(
    state: State<'_, RuntimeState>,
    tariff: f64,
    carbon: f64,
) -> Result<Value, String> {
    // Clamped, because a negative tariff would make the projection pay the user
    // and a zero carbon factor would claim the machine emits nothing.
    if !tariff.is_finite() || !carbon.is_finite() {
        return Err("Price and carbon factors must be finite numbers.".to_string());
    }
    let tariff = tariff.clamp(0.0, 5.0);
    let carbon = carbon.clamp(0.0, 5_000.0);
    let mut collector = state
        .collector
        .lock()
        .map_err(|_| "VISOR native collector is unavailable.".to_string())?;
    collector.set_price_factors(tariff, carbon);
    Ok(json!({ "tariffPerKwh": tariff, "carbonGramsPerKwh": carbon }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // A previous session's energy total and measured history continue rather
    // than reset: the file lives in the application-data directory next to
    // the settings store, and a missing file is simply a first launch.
    let mut collector = Collector::new();
    if let Some(record) = session::load() {
        collector.restore_session(record);
    }
    let state = RuntimeState {
        latest: Arc::new(RwLock::new(None)),
        alert_settings: Arc::new(Mutex::new(HashMap::new())),
        collector: Arc::new(Mutex::new(collector)),
        shell: Arc::new(ShellFlags {
            // Restarts must not silently begin sending system notifications.
            notifications: AtomicBool::new(false),
            ..ShellFlags::default()
        }),
    };
    tauri::Builder::default()
        // Registered first so a second launch focuses the running window instead of
        // starting a second collector.
        .plugin(tauri_plugin_single_instance::init(
            |app, _arguments, _directory| {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            },
        ))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // Size, place and zoom only. Restoring visibility or a fullscreen
                // state would fight the tray and the collector's own startup order.
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .build(),
        )
        // No launch arguments: nothing in the app parses a `--minimized` flag,
        // so passing one here would be a label with no behaviour behind it.
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            get_system_snapshot,
            refresh_system_snapshot,
            get_throughput_history,
            kill_process,
            set_process_priority,
            set_alert_rule,
            get_shell_state,
            set_shell_flags,
            set_price_factors
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let menu = shell::build_menu(&handle)?;
            app.set_menu(menu)?;
            shell::build_tray(app)?;
            start_collector(state.clone(), handle);
            Ok(())
        })
        .on_menu_event(shell::handle_menu_event)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<RuntimeState>();
                // Opt-in only: closing the window quits unless the user chose to keep
                // monitoring from the tray.
                if state.shell.close_to_tray.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running VISOR");
}
