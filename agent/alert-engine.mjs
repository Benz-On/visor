export const DEFAULT_ALERT_RULES = [
  { id: 'cpu-sustained', title: 'CPU sustained load', metric: 'cpu', threshold: 90, durationMs: 120_000, unit: '%', suppressDuringGaming: true, enabled: true, severity: 'warning' },
  { id: 'gpu-sustained', title: 'GPU sustained load', metric: 'gpu', threshold: 95, durationMs: 90_000, unit: '%', suppressDuringGaming: true, enabled: true, severity: 'warning' },
  { id: 'cpu-thermal', title: 'CPU temperature', metric: 'cpuTemp', threshold: 90, durationMs: 30_000, unit: '°C', suppressDuringGaming: false, enabled: true, severity: 'critical' },
  { id: 'gpu-thermal', title: 'GPU temperature', metric: 'gpuTemp', threshold: 86, durationMs: 30_000, unit: '°C', suppressDuringGaming: false, enabled: true, severity: 'critical' },
  { id: 'ssd-thermal', title: 'SSD temperature', metric: 'ssdTemp', threshold: 70, durationMs: 60_000, unit: '°C', suppressDuringGaming: false, enabled: true, severity: 'warning' },
  { id: 'vram-pressure', title: 'VRAM pressure', metric: 'vram', threshold: 95, durationMs: 30_000, unit: '%', suppressDuringGaming: false, enabled: true, severity: 'warning' },
];

const finite = (value) => Number.isFinite(Number(value)) ? Number(value) : 0;

export function createAlertEngine(initialRules = DEFAULT_ALERT_RULES) {
  const rules = new Map(initialRules.map((rule) => [rule.id, { ...rule }]));
  const states = new Map();
  const recent = [];

  function setEnabled(id, enabled) {
    const rule = rules.get(id);
    if (!rule) return false;
    rule.enabled = Boolean(enabled);
    if (!rule.enabled) states.delete(id);
    return true;
  }

  function evaluate(metrics = {}, gaming = { active: false }, now = Date.now()) {
    const active = [];
    const watch = [];

    for (const rule of rules.values()) {
      const value = finite(metrics[rule.metric]);
      const sensorAvailable = !/Temp$/.test(rule.metric) || value > 0;
      const suppressed = rule.enabled && rule.suppressDuringGaming && Boolean(gaming.active);
      const exceeded = rule.enabled && sensorAvailable && value >= rule.threshold && !suppressed;
      let state = states.get(rule.id);

      if (exceeded) {
        state ||= { since: now, active: false, peak: value };
        state.peak = Math.max(state.peak, value);
        state.active = now - state.since >= rule.durationMs;
        states.set(rule.id, state);
      } else {
        if (state?.active) {
          recent.unshift({
            id: `${rule.id}:${now}`,
            ruleId: rule.id,
            title: rule.title,
            peak: state.peak,
            unit: rule.unit,
            startedAt: new Date(state.since).toISOString(),
            resolvedAt: new Date(now).toISOString(),
            gamingSuppressed: suppressed,
          });
          recent.splice(6);
        }
        states.delete(rule.id);
        state = null;
      }

      const elapsedMs = state ? Math.max(0, now - state.since) : 0;
      const item = {
        ...rule,
        value,
        sensorAvailable,
        suppressed,
        elapsedMs,
        progress: state ? Math.min(100, Math.round(elapsedMs / rule.durationMs * 100)) : 0,
        active: Boolean(state?.active),
        since: state ? new Date(state.since).toISOString() : null,
      };
      if (item.active) active.push(item);
      else if (exceeded || suppressed) watch.push(item);
    }

    return {
      active,
      watch,
      recent: [...recent],
      gaming: {
        active: Boolean(gaming.active),
        processName: gaming.processName || null,
        pid: gaming.pid || null,
        suppressedRuleIds: gaming.active
          ? [...rules.values()].filter((rule) => rule.enabled && rule.suppressDuringGaming).map((rule) => rule.id)
          : [],
      },
      rules: [...rules.values()],
    };
  }

  return { evaluate, setEnabled };
}
