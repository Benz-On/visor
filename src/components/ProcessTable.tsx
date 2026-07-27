import { useEffect, useMemo, useState } from 'react';
import { AlertTriangle, ChevronDown, Gauge, Search, Shield, SlidersHorizontal, Trash2, Zap } from 'lucide-react';
import { processes as demoProcesses } from '../data';
import type { ProcessInfo } from '../types';

type SortKey = 'cpu' | 'gpu' | 'memory' | 'vram' | 'energy';

interface ProcessTableProps {
  expanded?: boolean;
  processes?: ProcessInfo[];
  live?: boolean;
  onKillProcess?: (pid: number) => Promise<unknown>;
  onSetPriority?: (pid: number, priority: 'low' | 'belowNormal' | 'normal' | 'aboveNormal' | 'high') => Promise<unknown>;
}

const powerClass: Record<ProcessInfo['power'], string> = {
  'Very low': 'power-low',
  Low: 'power-low',
  Moderate: 'power-medium',
  High: 'power-high',
};

export function ProcessTable({
  expanded = false,
  processes = demoProcesses,
  live = false,
  onKillProcess,
  onSetPriority,
}: ProcessTableProps) {
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<SortKey>('gpu');
  const [selected, setSelected] = useState<number | null>(processes[0]?.id ?? null);
  const [confirmProcess, setConfirmProcess] = useState<ProcessInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const visibleProcesses = useMemo(() => {
    return processes
      .filter((process) =>
        `${process.name} ${process.subtitle}`.toLowerCase().includes(query.toLowerCase()),
      )
      .sort((a, b) => {
        if (sort === 'energy') return (b.energyWatts || 0) - (a.energyWatts || 0);
        return b[sort] - a[sort];
      })
      .slice(0, expanded ? 80 : 5);
  }, [expanded, processes, query, sort]);
  const selectedProcess = processes.find((process) => process.id === selected) || null;

  useEffect(() => {
    if (expanded && !selectedProcess && processes.length > 0) setSelected(processes[0].id);
  }, [expanded, processes, selectedProcess]);

  const cycleSort = () => setSort((current) => current === 'gpu' ? 'cpu' : current === 'cpu' ? 'energy' : 'gpu');

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
          <h2>{expanded ? 'Process explorer' : 'Top consumers'} {live && <span className="native-data-badge">WINDOWS LIVE</span>}</h2>
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
          <button className="icon-button" aria-label="Filter processes">
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
                <td>{process.memory.toFixed(1)} GB</td>
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
          </tbody>
        </table>
      </div>
      {expanded && selectedProcess && (
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
      )}
      {notice && <button className="process-notice" onClick={() => setNotice(null)}>{notice}<span>Dismiss</span></button>}
      {!expanded && (
        <div className="panel-footer">
          <span>{processes.length} sampled processes · {processes.reduce((sum, process) => sum + (process.threads || 0), 0).toLocaleString()} threads</span>
          <button className="text-button">View all processes</button>
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
