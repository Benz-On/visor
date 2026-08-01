import { useMemo, useState } from 'react';
import { AlertTriangle, ChevronDown, Gauge, RefreshCw, Search, Shield, SlidersHorizontal, Trash2, Zap } from 'lucide-react';
import { processes as demoProcesses } from '../data';
import type { ProcessInfo } from '../types';

type SortKey = 'cpu' | 'gpu' | 'memory' | 'vram' | 'energy';

interface ProcessTableProps {
  expanded?: boolean;
  processes?: ProcessInfo[];
  live?: boolean;
  refreshing?: boolean;
  onRefresh?: () => Promise<unknown>;
  onViewAll?: () => void;
  onKillProcess?: (pid: number) => Promise<unknown>;
  onSetPriority?: (pid: number, priority: 'low' | 'belowNormal' | 'normal' | 'aboveNormal' | 'high') => Promise<unknown>;
}

const powerClass: Record<ProcessInfo['power'], string> = {
  'Very low': 'power-low',
  Low: 'power-low',
  Moderate: 'power-medium',
  High: 'power-high',
};

const formatBytes = (bytes = 0) => {
  if (!bytes) return '0 MB';
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
  return `${Math.max(0.1, bytes / 1024 ** 2).toFixed(bytes >= 100 * 1024 ** 2 ? 0 : 1)} MB`;
};

export function ProcessTable({
  expanded = false,
  processes = demoProcesses,
  live = false,
  refreshing = false,
  onRefresh,
  onViewAll,
  onKillProcess,
  onSetPriority,
}: ProcessTableProps) {
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<SortKey>(expanded ? 'memory' : 'energy');
  const [appsOnly, setAppsOnly] = useState(false);
  const [selected, setSelected] = useState<number | null>(processes[0]?.id ?? null);
  const [confirmProcess, setConfirmProcess] = useState<ProcessInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const visibleProcesses = useMemo(() => {
    return processes
      .filter((process) =>
        (!appsOnly || !process.protected)
        && `${process.name} ${process.subtitle} ${process.path || ''}`.toLowerCase().includes(query.toLowerCase()),
      )
      .sort((a, b) => {
        if (sort === 'energy') return (b.energyWatts || 0) - (a.energyWatts || 0);
        return b[sort] - a[sort];
      })
      .slice(0, expanded ? 80 : 5);
  }, [appsOnly, expanded, processes, query, sort]);
  const selectedProcess = processes.find((process) => process.id === selected) || (expanded ? processes[0] : null);

  const cycleSort = () => {
    const order: SortKey[] = ['memory', 'cpu', 'gpu', 'vram', 'energy'];
    setSort((current) => order[(order.indexOf(current) + 1) % order.length]);
  };

  const changePriority = async (priority: 'belowNormal' | 'normal' | 'high') => {
    if (!selectedProcess || !onSetPriority) return;
    setBusy(true);
    try {
      await onSetPriority(selectedProcess.id, priority);
      setNotice(`${selectedProcess.name} priority changed to ${priority}.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Priority change failed.');
    } finally {
      setBusy(false);
    }
  };

  const confirmKill = async () => {
    if (!confirmProcess || !onKillProcess) return;
    setBusy(true);
    try {
      await onKillProcess(confirmProcess.id);
      setNotice(`${confirmProcess.name} and its process tree were ended.`);
      setSelected(null);
      setConfirmProcess(null);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Process termination failed.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className={`panel process-panel ${expanded ? 'process-panel-expanded' : ''}`}>
      <div className="panel-header process-header">
        <div>
          <p className="eyebrow">RESOURCE ATTRIBUTION</p>
          <h2>{expanded ? 'Process explorer' : 'Top consumers'} {live && <span className="native-data-badge">LOCAL LIVE</span>}</h2>
        </div>
        <div className="table-tools">
          {expanded && (
            <label className="inline-search">
              <Search size={15} />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search processes"
                aria-label="Search processes"
              />
            </label>
          )}
          {expanded && <button className="ghost-button refresh-button" disabled={!live || refreshing} onClick={() => void onRefresh?.()}><RefreshCw className={refreshing ? 'spin' : ''} size={15} />{refreshing ? 'Refreshing' : 'Refresh RAM'}</button>}
          <button className={`icon-button ${appsOnly ? 'active-filter' : ''}`} aria-label="Show applications only" aria-pressed={appsOnly} onClick={() => setAppsOnly((value) => !value)} title="Hide protected system processes">
            <SlidersHorizontal size={17} />
          </button>
          <button className="ghost-button sort-button" onClick={cycleSort}>
            Sort: {sort === 'energy' ? 'ENERGY' : sort.toUpperCase()} <ChevronDown size={14} />
          </button>
        </div>
      </div>

      <div className="process-table-wrap">
        <table className="process-table">
          <thead>
            <tr>
              <th>Application</th>
              <th>CPU</th>
              <th>GPU</th>
              <th>Memory</th>
              <th>VRAM</th>
              {expanded && <th>Disk</th>}
              {expanded && <th>Network</th>}
              {expanded && <th>Energy</th>}
              <th>Impact</th>
              <th aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {visibleProcesses.map((process) => (
              <tr
                key={process.id}
                className={selected === process.id ? 'selected-row' : ''}
                onClick={() => setSelected(process.id)}
              >
                <td>
                  <div className="process-name-cell">
                    <span className="app-icon" style={{ background: process.color }}>
                      {process.icon}
                    </span>
                    <div>
                      <div className="process-name-line">
                        <strong>{process.name}</strong>
                        {process.kind && <span className={`kind-badge kind-${process.kind.toLowerCase()}`}>{process.kind}</span>}
                      </div>
                      <span>{process.subtitle}</span>
                    </div>
                  </div>
                </td>
                <td>{process.cpu.toFixed(1)}%</td>
                <td>
                  <div className="heat-cell" style={{ '--heat': `${Math.min(process.gpu / 50, 0.92)}` } as React.CSSProperties}>
                    {process.gpu.toFixed(1)}%
                  </div>
                </td>
                <td><strong className="memory-value">{formatBytes(process.workingSetBytes || process.memory * 1024 ** 3)}</strong><small className="memory-percent">{(process.memoryPercent || 0).toFixed(1)}%</small></td>
                <td>{process.vram.toFixed(1)} GB</td>
                {expanded && <td>{process.disk.toFixed(1)} MB/s</td>}
                {expanded && <td>{process.network.toFixed(1)} Mbps</td>}
                {expanded && <td><span className="energy-cell"><Zap size={11} />~{(process.energyWatts || 0).toFixed(1)} W</span></td>}
                <td><span className={`power-badge ${powerClass[process.power]}`}>{process.power}</span></td>
                <td>
                  <button className="row-menu" aria-label={`Select ${process.name}`} onClick={(event) => { event.stopPropagation(); setSelected(process.id); }}>
                    <Gauge size={15} />
                  </button>
                </td>
              </tr>
            ))}
            {visibleProcesses.length === 0 && <tr><td className="process-empty" colSpan={expanded ? 10 : 7}>No process matches this view.</td></tr>}
          </tbody>
        </table>
      </div>
      {expanded && selectedProcess && (
        <div className="process-inspector">
          <div className="process-control-bar">
            <div className="process-control-identity">
              <span className="app-icon" style={{ background: selectedProcess.color }}>{selectedProcess.icon}</span>
              <div><strong>{selectedProcess.name}</strong><span>PID {selectedProcess.id} · {selectedProcess.threads || 0} threads · {selectedProcess.handles || 0} handles</span></div>
            </div>
            <div className="process-control-energy"><Zap size={15} /><span>Attributed now</span><strong>~{(selectedProcess.energyWatts || 0).toFixed(1)} W</strong></div>
            <div className="process-control-actions">
              {selectedProcess.protected ? (
                <span className="protected-process"><Shield size={14} /> System protected</span>
              ) : (
                <>
                  <button className="ghost-button" disabled={!live || busy} onClick={() => void changePriority('belowNormal')}>Efficiency mode</button>
                  <button className="ghost-button" disabled={!live || busy} onClick={() => void changePriority('high')}>High priority</button>
                  <button className="danger-button" disabled={!live || busy} onClick={() => setConfirmProcess(selectedProcess)}><Trash2 size={14} /> End task</button>
                </>
              )}
            </div>
          </div>
          <div className="process-detail-grid">
            <div><span>Working set</span><strong>{formatBytes(selectedProcess.workingSetBytes || selectedProcess.memory * 1024 ** 3)}</strong><small>Physical RAM in use</small></div>
            <div><span>Private memory</span><strong>{formatBytes(selectedProcess.privateBytes)}</strong><small>Exclusive allocation</small></div>
            <div><span>Virtual memory</span><strong>{formatBytes(selectedProcess.virtualBytes)}</strong><small>Address space</small></div>
            <div><span>Disk now</span><strong>{(selectedProcess.disk || 0).toFixed(2)} MB/s</strong><small>{(selectedProcess.diskRead || 0).toFixed(2)} read · {(selectedProcess.diskWrite || 0).toFixed(2)} write</small></div>
            <div><span>State</span><strong>{selectedProcess.responding === false ? 'Not responding' : selectedProcess.state || 'Unknown'}</strong><small>Parent PID {selectedProcess.parentId || '—'}</small></div>
            <div className="process-path-detail"><span>Executable</span><strong title={selectedProcess.path}>{selectedProcess.path || 'Path protected by operating system'}</strong><small>{selectedProcess.startedAt ? `Started ${new Date(selectedProcess.startedAt).toLocaleString()}` : `${selectedProcess.uptimeSeconds || 0}s observed uptime`}</small></div>
          </div>
        </div>
      )}
      {notice && <button className="process-notice" onClick={() => setNotice(null)}>{notice}<span>Dismiss</span></button>}
      {!expanded && (
        <div className="panel-footer">
          <span>{processes.length} sampled processes · {processes.reduce((sum, process) => sum + (process.threads || 0), 0).toLocaleString()} threads</span>
          <button className="text-button" onClick={onViewAll}>View all processes</button>
        </div>
      )}
      {confirmProcess && (
        <div className="confirm-backdrop" onMouseDown={() => !busy && setConfirmProcess(null)}>
          <div className="confirm-dialog" onMouseDown={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-label="Confirm process termination">
            <div className="confirm-icon"><AlertTriangle size={21} /></div>
            <p className="eyebrow">DESTRUCTIVE ACTION</p>
            <h3>End {confirmProcess.name}?</h3>
            <p>VISOR will terminate PID {confirmProcess.id} and its child processes. Unsaved work in this application may be lost.</p>
            <div className="kill-impact"><Zap size={15} /><span>Estimated power released</span><strong>~{(confirmProcess.energyWatts || 0).toFixed(1)} W</strong></div>
            <div className="confirm-actions"><button className="ghost-button" disabled={busy} onClick={() => setConfirmProcess(null)}>Cancel</button><button className="danger-button" disabled={busy} onClick={() => void confirmKill()}>{busy ? 'Ending…' : 'End process tree'}</button></div>
          </div>
        </div>
      )}
    </section>
  );
}
