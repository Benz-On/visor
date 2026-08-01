import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { processes as demoProcesses } from '../data';
import type { LiveMetrics, MetricKey, SystemSnapshot } from '../types';
import { useTelemetry } from './useTelemetry';

const AGENT_URL = 'http://127.0.0.1:1421';
const historyKeys: Array<MetricKey | 'network' | 'disk'> = ['cpu', 'gpu', 'ram', 'vram', 'network', 'disk'];
const emptyHistory = () => Object.fromEntries(historyKeys.map((key) => [key, Array(34).fill(0)])) as LiveMetrics['history'];
const emptyMetrics: LiveMetrics = {
  cpu: 0,
  gpu: 0,
  ram: 0,
  vram: 0,
  cpuTemp: 0,
  gpuTemp: 0,
  cpuPower: 0,
  gpuPower: 0,
  download: 0,
  upload: 0,
  diskRead: 0,
  diskWrite: 0,
  history: emptyHistory(),
};

type ConnectionState = 'connecting' | 'live' | 'demo' | 'error';

const isTauri = () => '__TAURI_INTERNALS__' in window;

export function useVisorData(paused: boolean) {
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [connection, setConnection] = useState<ConnectionState>('connecting');
  const [actionError, setActionError] = useState<string | null>(null);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [history, setHistory] = useState<LiveMetrics['history'] | null>(null);
  const connectedRef = useRef(false);
  const nativeRuntime = isTauri();
  const fallback = useTelemetry(paused || nativeRuntime || connection === 'live');
  const fetchSnapshot = useCallback(async (signal?: AbortSignal, force = false) => {
    try {
      const next = nativeRuntime
        ? await invoke<SystemSnapshot>(force ? 'refresh_system_snapshot' : 'get_system_snapshot')
        : await fetch(`${AGENT_URL}/api/snapshot`, { cache: 'no-store', signal }).then(async (response) => {
          if (!response.ok) throw new Error(`Agent returned ${response.status}`);
          return response.json() as Promise<SystemSnapshot>;
        });
      setSnapshot(next);
      setConnection('live');
      setConnectionError(null);
      connectedRef.current = true;
      setHistory((current) => {
        const base = current || (nativeRuntime ? emptyHistory() : fallback.history);
        const nextValues = {
          cpu: next.metrics.cpu,
          gpu: next.metrics.gpu,
          ram: next.metrics.ram,
          vram: next.metrics.vram,
          network: Math.min(100, next.metrics.download * 1.5),
          disk: Math.min(100, next.metrics.diskActivity || (next.metrics.diskRead + next.metrics.diskWrite) / 4),
        };
        return Object.fromEntries(historyKeys.map((key) => [key, [...base[key].slice(-33), nextValues[key]]])) as LiveMetrics['history'];
      });
      return next;
    } catch (error) {
      if (error instanceof DOMException && error.name === 'AbortError') return null;
      setConnection(connectedRef.current || nativeRuntime ? 'error' : 'demo');
      setConnectionError(error instanceof Error ? error.message : String(error));
      return null;
    }
  }, [fallback.history, nativeRuntime]);

  useEffect(() => {
    const controller = new AbortController();
    const initialTimer = window.setTimeout(() => void fetchSnapshot(controller.signal), 0);
    if (paused) return () => {
      controller.abort();
      window.clearTimeout(initialTimer);
    };
    const timer = window.setInterval(() => void fetchSnapshot(controller.signal), 1100);
    return () => {
      controller.abort();
      window.clearTimeout(initialTimer);
      window.clearInterval(timer);
    };
  }, [fetchSnapshot, paused]);

  const performAction = useCallback(async (path: string, body: Record<string, unknown>) => {
    setActionError(null);
    if (nativeRuntime) {
      let result: { ok?: boolean; error?: string };
      if (path.endsWith('/kill')) {
        result = await invoke('kill_process', {
          pid: Number(path.split('/')[3]),
          confirmation: String(body.confirmation),
          tree: body.tree !== false,
          force: body.force === true,
        });
      } else if (path.endsWith('/priority')) {
        result = await invoke('set_process_priority', {
          pid: Number(path.split('/')[3]),
          priority: body.priority,
        });
      } else if (path.startsWith('/api/alerts/')) {
        const alertId = path.split('/').pop();
        result = await invoke('set_alert_rule', {
          id: alertId,
          enabled: Boolean(body.enabled),
        });
      } else {
        throw new Error('Unsupported native VISOR action.');
      }
      await fetchSnapshot(undefined, true);
      return result;
    }
    const response = await fetch(`${AGENT_URL}${path}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Visor-Action': 'confirmed',
      },
      body: JSON.stringify(body),
    });
    const result = await response.json() as { ok?: boolean; error?: string };
    if (!response.ok || !result.ok) {
      const message = result.error || 'The operating system action failed.';
      setActionError(message);
      throw new Error(message);
    }
    await fetchSnapshot();
    return result;
  }, [fetchSnapshot, nativeRuntime]);

  const killProcess = useCallback((pid: number) => performAction(`/api/processes/${pid}/kill`, {
    confirmation: String(pid),
    force: true,
    tree: true,
  }), [performAction]);

  const setProcessPriority = useCallback((pid: number, priority: 'low' | 'belowNormal' | 'normal' | 'aboveNormal' | 'high') =>
    performAction(`/api/processes/${pid}/priority`, { priority }), [performAction]);

  const setAlertRule = useCallback((id: string, enabled: boolean) =>
    performAction(`/api/alerts/${id}`, { enabled }), [performAction]);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      return await fetchSnapshot(undefined, true);
    } finally {
      setRefreshing(false);
    }
  }, [fetchSnapshot]);

  const metrics = useMemo<LiveMetrics>(() => snapshot ? {
    ...fallback,
    ...snapshot.metrics,
    history: history || fallback.history,
  } : nativeRuntime ? emptyMetrics : fallback, [fallback, history, nativeRuntime, snapshot]);

  return {
    metrics,
    snapshot,
    connection,
    connectionError,
    actionError,
    refreshing,
    processes: snapshot?.processes || (nativeRuntime ? [] : demoProcesses),
    killProcess,
    setProcessPriority,
    setAlertRule,
    refresh,
  };
}
