import type { ThemeId } from './types';

/**
 * Settings are persisted through the Tauri store plugin, which writes into the
 * application data directory. The browser fallback (the development agent, and
 * `npm run dev` in a plain browser tab) has no plugin host, so it keeps using
 * localStorage instead of failing at start-up.
 */
const STORE_FILE = 'settings.json';
const STORE_KEY = 'settings';
const LEGACY_STORAGE_KEY = 'visor-theme';
const nativeRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export interface VisorSettings {
  theme: ThemeId;
  tariffPerKwh: number;
  carbonGramsPerKwh: number;
  notifications: boolean;
  closeToTray: boolean;
  autostart: boolean;
  /** Sustained-rule overrides that survive a restart, keyed by rule id. */
  alertRules: Record<string, boolean>;
}

export const defaultSettings: VisorSettings = {
  theme: 'studio',
  tariffPerKwh: 0.25,
  carbonGramsPerKwh: 56,
  // Restarts must not begin sending system notifications nobody opted into.
  notifications: false,
  closeToTray: false,
  autostart: false,
  alertRules: {},
};

const themes: ThemeId[] = ['studio', 'porcelain', 'cyber', 'retro'];

const clamp = (value: unknown, fallback: number, minimum: number, maximum: number) => {
  const number = Number(value);
  if (!Number.isFinite(number)) return fallback;
  return Math.min(maximum, Math.max(minimum, number));
};

const KNOWN_ALERT_RULES = [
  'cpu-sustained',
  'gpu-sustained',
  'cpu-thermal',
  'gpu-thermal',
  'ssd-thermal',
  'vram-pressure',
];

function sanitizeAlertRules(saved: unknown): Record<string, boolean> {
  if (typeof saved !== 'object' || saved === null) return {};
  const rules: Record<string, boolean> = {};
  for (const [id, value] of Object.entries(saved as Record<string, unknown>)) {
    if (KNOWN_ALERT_RULES.includes(id) && typeof value === 'boolean') {
      rules[id] = value;
    }
  }
  return rules;
}

function sanitize(saved: Partial<VisorSettings> | null | undefined): VisorSettings {
  const stored = saved ?? {};
  return {
    theme: themes.includes(stored.theme as ThemeId) ? (stored.theme as ThemeId) : defaultSettings.theme,
    tariffPerKwh: clamp(stored.tariffPerKwh, defaultSettings.tariffPerKwh, 0, 5),
    carbonGramsPerKwh: clamp(stored.carbonGramsPerKwh, defaultSettings.carbonGramsPerKwh, 0, 5000),
    notifications: typeof stored.notifications === 'boolean' ? stored.notifications : defaultSettings.notifications,
    closeToTray: typeof stored.closeToTray === 'boolean' ? stored.closeToTray : defaultSettings.closeToTray,
    autostart: typeof stored.autostart === 'boolean' ? stored.autostart : defaultSettings.autostart,
    alertRules: sanitizeAlertRules(stored.alertRules),
  };
}

async function openStore() {
  const { load } = await import('@tauri-apps/plugin-store');
  return load(STORE_FILE, { autoSave: true });
}

export async function loadSettings(): Promise<VisorSettings> {
  if (nativeRuntime()) {
    try {
      const store = await openStore();
      return sanitize(await store.get<Partial<VisorSettings>>(STORE_KEY));
    } catch {
      // A missing or unreadable file is a first launch, not an error to show.
      return { ...defaultSettings };
    }
  }
  try {
    const saved = window.localStorage.getItem(STORE_KEY);
    const legacy = window.localStorage.getItem(LEGACY_STORAGE_KEY);
    const parsed = saved ? (JSON.parse(saved) as Partial<VisorSettings>) : {};
    // The theme was the only stored setting before settings existed, so honour it
    // rather than resetting a returning browser-mode user to Studio.
    if (!parsed.theme && legacy && themes.includes(legacy as ThemeId)) parsed.theme = legacy as ThemeId;
    return sanitize(parsed);
  } catch {
    return { ...defaultSettings };
  }
}

export async function saveSettings(settings: VisorSettings): Promise<void> {
  if (nativeRuntime()) {
    try {
      const store = await openStore();
      await store.set(STORE_KEY, settings);
      await store.save();
      return;
    } catch {
      // fall through so the change is not silently lost
    }
  }
  try {
    window.localStorage.setItem(STORE_KEY, JSON.stringify(settings));
    window.localStorage.setItem(LEGACY_STORAGE_KEY, settings.theme);
  } catch {
    // Private-mode browsers refuse storage; the session still works unsaved.
  }
}
