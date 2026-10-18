/**
 * Bridges between the React shell and the Tauri window, so every native call
 * degrades quietly when the interface runs in a plain browser tab against the
 * development agent. Without these guards the plugin imports themselves would
 * throw and the app would fail to mount outside Tauri.
 */

const nativeRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const isDesktop = nativeRuntime;

export type ShellAction =
  | { action: 'navigate'; view: string }
  | { action: 'search' }
  | { action: 'refresh' }
  | { action: 'toggle-pause' };

export type ShellFlags = {
  paused: boolean;
  notifications: boolean;
  closeToTray: boolean;
};

/// The native menu bar and the tray icon are Rust surfaces, so their actions
/// arrive as one event rather than as clicks on an element.
export async function subscribeShell(handle: (action: ShellAction) => void): Promise<() => void> {
  if (!nativeRuntime()) return () => {};
  try {
    const { listen } = await import('@tauri-apps/api/event');
    return await listen<ShellAction>('visor://shell', (event) => handle(event.payload));
  } catch {
    // A window without the event API still works; menu items simply do nothing.
    return () => {};
  }
}

/// Arguments stay camelCase because Tauri maps them onto the command's
/// snake_case signature; sending `close_to_tray` would arrive as unknown.
export async function applyShellFlags(flags: ShellFlags): Promise<void> {
  if (!nativeRuntime()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_shell_flags', flags);
  } catch {
    // The tray tooltip and the notification switch are the only casualties.
  }
}

/// False means the running collector could not take the new figures, which is
/// how the interface knows to label a projection as browser-side only.
export async function applyPriceFactors(tariff: number, carbon: number): Promise<boolean> {
  if (!nativeRuntime()) return false;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_price_factors', { tariff, carbon });
    return true;
  } catch {
    return false;
  }
}

export async function applyAutostart(enabled: boolean): Promise<boolean> {
  if (!nativeRuntime()) return false;
  try {
    const autostart = await import('@tauri-apps/plugin-autostart');
    if (enabled) await autostart.enable();
    else await autostart.disable();
    return true;
  } catch {
    return false;
  }
}
