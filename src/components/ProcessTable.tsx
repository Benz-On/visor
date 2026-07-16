import { useMemo, useState } from 'react';
import { ChevronDown, MoreHorizontal, Search, SlidersHorizontal } from 'lucide-react';
import { processes } from '../data';
import type { ProcessInfo } from '../types';

type SortKey = 'cpu' | 'gpu' | 'memory' | 'vram';

interface ProcessTableProps {
  expanded?: boolean;
}

const powerClass: Record<ProcessInfo['power'], string> = {
  'Very low': 'power-low',
  Low: 'power-low',
  Moderate: 'power-medium',
  High: 'power-high',
};

export function ProcessTable({ expanded = false }: ProcessTableProps) {
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<SortKey>('gpu');
  const [selected, setSelected] = useState<number | null>(processes[0].id);

  const visibleProcesses = useMemo(() => {
    return processes
      .filter((process) =>
        `${process.name} ${process.subtitle}`.toLowerCase().includes(query.toLowerCase()),
      )
      .sort((a, b) => b[sort] - a[sort])
      .slice(0, expanded ? undefined : 5);
  }, [expanded, query, sort]);

  return (
    <section className={`panel process-panel ${expanded ? 'process-panel-expanded' : ''}`}>
      <div className="panel-header process-header">
        <div>
          <p className="eyebrow">RESOURCE ATTRIBUTION</p>
          <h2>{expanded ? 'Process explorer' : 'Top consumers'}</h2>
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
          <button className="ghost-button sort-button" onClick={() => setSort(sort === 'gpu' ? 'cpu' : 'gpu')}>
            Sort: {sort.toUpperCase()} <ChevronDown size={14} />
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
                <td><span className={`power-badge ${powerClass[process.power]}`}>{process.power}</span></td>
                <td>
                  <button className="row-menu" aria-label={`Actions for ${process.name}`} onClick={(event) => event.stopPropagation()}>
                    <MoreHorizontal size={17} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {!expanded && (
        <div className="panel-footer">
          <span>212 processes · 3,847 threads</span>
          <button className="text-button">View all processes</button>
        </div>
      )}
    </section>
  );
}
