//! Desktop shell: the native menu bar, the tray icon and OS notifications.
//!
//! These surfaces exist because a system monitor is expected to keep working
//! while its window is not focused. Menu and tray actions are translated into
//! one event on the webview rather than new commands, so the frontend has a
//! single place to handle "the user asked for this from outside the window".

use serde_json::{json, Value};
use std::collections::HashSet;
use tauri::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, Wry};
use tauri_plugin_notification::NotificationExt;

/// Event name the frontend listens to for every menu and tray action.
pub const SHELL_EVENT: &str = "visor://shell";
pub const TRAY_ID: &str = "visor-tray";

const VIEWS: [(&str, &str, &str); 9] = [
    ("overview", "Overview", "CmdOrCtrl+1"),
    ("hardware", "Hardware", "CmdOrCtrl+2"),
    ("processes", "Processes", "CmdOrCtrl+3"),
    ("performance", "Performance", "CmdOrCtrl+4"),
    ("ai", "AI workloads", "CmdOrCtrl+5"),
    ("energy", "Energy lens", "CmdOrCtrl+6"),
    ("history", "History", "CmdOrCtrl+7"),
    ("alerts", "Alerts", "CmdOrCtrl+8"),
    ("settings", "Settings", "CmdOrCtrl+9"),
];

fn navigate_id(view: &str) -> String {
    format!("view.{view}")
}

pub fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let mut view_items = Vec::with_capacity(VIEWS.len());
    for (view, title, accelerator) in VIEWS {
        view_items.push(MenuItem::with_id(
            app,
            navigate_id(view),
            title,
            true,
            Some(accelerator),
        )?);
    }

    let search = MenuItem::with_id(
        app,
        "shell.search",
        "Search and commands…",
        true,
        Some("CmdOrCtrl+K"),
    )?;
    // No F5: WebView2 reloads the page on that key itself, and a menu item that
    // silently does something else would be worse than no shortcut at all.
    let refresh = MenuItem::with_id(
        app,
        "shell.refresh",
        "Refresh telemetry",
        true,
        None::<&str>,
    )?;
    // Ctrl+P is left out for the same reason: the webview would open a print
    // dialog instead of pausing.
    let pause = MenuItem::with_id(
        app,
        "shell.pause",
        "Pause or resume monitoring",
        true,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(
        app,
        "shell.settings",
        "Settings…",
        true,
        Some("CmdOrCtrl+,"),
    )?;
    let quit = PredefinedMenuItem::quit(app, Some("Quit VISOR"))?;

    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &refresh,
            &pause,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let mut view_children: Vec<&dyn IsMenuItem<Wry>> = Vec::with_capacity(view_items.len() + 2);
    for item in &view_items {
        view_children.push(item);
    }
    let view_separator = PredefinedMenuItem::separator(app)?;
    view_children.push(&view_separator);
    view_children.push(&search);
    let view = Submenu::with_items(app, "View", true, &view_children)?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some("Minimize"))?,
            &PredefinedMenuItem::maximize(app, Some("Zoom"))?,
        ],
    )?;

    Menu::with_items(app, &[&file, &view, &window])
}

pub fn build_tray(app: &mut App) -> tauri::Result<()> {
    let handle = app.handle();
    let show = MenuItem::with_id(handle, "tray.show", "Show VISOR", true, None::<&str>)?;
    let hide = MenuItem::with_id(handle, "tray.hide", "Hide window", true, None::<&str>)?;
    let quit = MenuItem::with_id(handle, "tray.quit", "Quit VISOR", true, None::<&str>)?;
    let menu = Menu::with_items(
        handle,
        &[&show, &hide, &PredefinedMenuItem::separator(handle)?, &quit],
    )?;

    // A bundle without an icon cannot own a tray; the window and menu still work,
    // so this stays a silent degradation instead of a startup failure.
    let Some(icon) = handle.default_window_icon().cloned() else {
        return Ok(());
    };

    TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("VISOR — system intelligence")
        .icon(icon)
        .on_tray_icon_event(|tray, event| {
            // A left click on the tray icon is the fastest way back to a hidden
            // window, and toggling matches how other tray-resident tools behave.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(handle)?;
    Ok(())
}

fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    // The tray menu owns its own window actions: showing and hiding a window is
    // a native call, so routing it through the webview would need the window to
    // be visible to bring it back.
    if let Some(view) = id.strip_prefix("view.") {
        let _ = app.emit(SHELL_EVENT, json!({ "action": "navigate", "view": view }));
        return;
    }
    let payload = match id {
        "shell.search" => json!({ "action": "search" }),
        "shell.refresh" => json!({ "action": "refresh" }),
        "shell.pause" => json!({ "action": "toggle-pause" }),
        "shell.settings" => json!({ "action": "navigate", "view": "settings" }),
        "tray.show" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            return;
        }
        "tray.hide" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
            return;
        }
        "tray.quit" => {
            app.exit(0);
            return;
        }
        _ => return,
    };
    let _ = app.emit(SHELL_EVENT, payload);
}

/// The tray tooltip is the only live figure visible without opening the window,
/// so it carries the headline numbers and the paused state.
pub fn update_tray(app: &AppHandle, tooltip: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

fn number(snapshot: &Value, path: &str) -> Option<f64> {
    let (head, key) = path.split_once('.')?;
    snapshot.get(head)?.get(key)?.as_f64().or_else(|| {
        snapshot
            .get(head)?
            .get(key)
            .and_then(Value::as_i64)
            .map(|v| v as f64)
    })
}

/// Renders a possibly-absent figure as an em dash rather than a zero.
fn f64(value: Option<f64>) -> String {
    value
        .map(|v| v.round().to_string())
        .unwrap_or_else(|| "—".to_string())
}

pub fn tooltip(snapshot: &Value, paused: bool) -> String {
    if paused {
        return "VISOR — updates paused".to_string();
    }
    let temp = number(snapshot, "metrics.cpuTemp");
    format!(
        "VISOR — CPU {}% · GPU {}% · {} W{}",
        f64(number(snapshot, "metrics.cpu")),
        f64(number(snapshot, "metrics.gpu")),
        f64(number(snapshot, "energy.watts")),
        temp.map(|v| format!(" · {:.0}°C", v)).unwrap_or_default(),
    )
}

/// Alerts already fired in this session, so a notification repeats only after a
/// condition clears and comes back rather than every second it stays true.
pub fn fresh_alerts(snapshot: &Value, seen: &mut HashSet<String>) -> Vec<(String, String)> {
    let mut fresh = Vec::new();
    let Some(active) = snapshot.get("alerts").and_then(|v| v.get("active")) else {
        seen.clear();
        return fresh;
    };
    let mut current = HashSet::new();
    if let Some(items) = active.as_array() {
        for item in items {
            let Some(id) = item.get("id").and_then(Value::as_str) else {
                continue;
            };
            current.insert(id.to_string());
            if seen.contains(id) {
                continue;
            }
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("VISOR alert")
                .to_string();
            let body = format!(
                "{}{} — VISOR keeps monitoring while the window is closed.",
                title,
                match (
                    item.get("value").and_then(Value::as_f64),
                    item.get("unit").and_then(Value::as_str),
                ) {
                    (Some(value), Some(unit)) => format!(" is at {:.1}{}", value, unit),
                    (Some(value), None) => format!(" is at {:.1}", value),
                    _ => String::new(),
                }
            );
            fresh.push((title, body));
        }
    }
    // A condition that dropped out must be able to notify again later.
    seen.retain(|id| current.contains(id));
    for id in &current {
        seen.insert(id.clone());
    }
    fresh
}
