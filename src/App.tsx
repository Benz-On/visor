import { useEffect, useMemo, useState } from 'react';
import {
  Activity,
  Bell,
  Bot,
  Check,
  ChevronDown,
  CircleGauge,
  Clock3,
  Command,
  Cpu,
  Download,
  Gauge,
  HardDrive,
  Info,
  Leaf,
  Maximize2,
  MemoryStick,
  Moon,
  Network,
  Pause,
  Play,
  Plus,
  Search,
  ShieldCheck,
  Sparkles,
  Sun,
  Thermometer,
  X,
  Zap,
} from 'lucide-react';
import { Donut, LineChart } from './components/Charts';
import { ProcessTable } from './components/ProcessTable';
import { Sidebar, type ViewId } from './components/Sidebar';
import { useVisorData } from './hooks/useVisorData';
import type { AgentInfo, EnergyEstimate, HardwareInfo, MetricKey, MetricTone, ProcessInfo } from './types';

const viewTitles: Record<ViewId, { eyebrow: string; title: string; description: string }> = {
  overview: {
    eyebrow: 'SYSTEM OVERVIEW',
    title: 'Good morning, Alex.',
    description: 'Live hardware, processes and energy in one calm view.',
  },
  processes: {
    eyebrow: 'PROCESS EXPLORER',
    title: 'Every process, clearly attributed.',
    description: 'See exactly where your resources are going.',
  },
  performance: {
    eyebrow: 'PERFORMANCE',
    title: 'Your hardware, in motion.',
    description: 'Live utilization, thermals and throughput.',
  },
  ai: {
    eyebrow: 'AI WORKLOADS',
    title: 'Local intelligence, understood.',
    description: 'Model activity, memory footprint and inference speed.',
  },
  energy: {
    eyebrow: 'ENERGY LENS',
    title: 'Every watt, made visible.',
    description: 'Measured power, modeled consumption and application impact.',
  },
  history: {
    eyebrow: 'HISTORY',
    title: 'A clear view of what happened.',
    description: 'Explore performance trends and peaks over time.',
  },
  alerts: {
    eyebrow: 'SMART ALERTS',
    title: 'Quiet when all is well.',
    description: 'VISOR only gets your attention when it matters.',
  },
  settings: {
    eyebrow: 'SETTINGS',
    title: 'Make VISOR yours.',
    description: 'Tune monitoring, appearance and notifications.',
  },
};

function App() {
  const [activeView, setActiveView] = useState<ViewId>('overview');
  const [collapsed, setCollapsed] = useState(false);
  const [paused, setPaused] = useState(false);
  const [theme, setTheme] = useState<'dark' | 'light'>('dark');
  const [commandOpen, setCommandOpen] = useState(false);
  const visor = useVisorData(paused);
  const metrics = visor.metrics;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setCommandOpen((open) => !open);
      }
      if (event.key === 'Escape') setCommandOpen(false);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const title = viewTitles[activeView];
  const thermalAlert = metrics.cpuTemp >= 90 || metrics.gpuTemp >= 86;
  const healthLabel = visor.connection === 'demo'
    ? 'Demo mode'
    : visor.connection === 'error'
      ? 'Reconnecting'
      : visor.connection === 'connecting'
        ? 'Connecting'
        : thermalAlert
          ? 'Attention'
          : 'Telemetry active';

  return (
    <div className="app-shell" data-theme={theme}>
      <Sidebar
        active={activeView}
        collapsed={collapsed}
        onNavigate={setActiveView}
        onCollapse={() => setCollapsed((value) => !value)}
        connection={visor.connection}
      />

      <main className="main-content">
        <header className="topbar">
          <button className="search-trigger" onClick={() => setCommandOpen(true)}>
            <Search size={17} />
            <span>Search processes, hardware, settings…</span>
            <kbd>Ctrl K</kbd>
          </button>
          <div className="topbar-actions">
            <div className="live-pill">
              <span className={paused ? 'paused-dot' : 'live-dot'} />
              {paused ? 'Paused' : 'Live'}
            </div>
            <button className="icon-button" onClick={() => setPaused((value) => !value)} aria-label={paused ? 'Resume monitoring' : 'Pause monitoring'}>
              {paused ? <Play size={17} /> : <Pause size={17} />}
            </button>
            <button
              className="icon-button"
              onClick={() => setTheme((value) => (value === 'dark' ? 'light' : 'dark'))}
              aria-label="Toggle theme"
            >
              {theme === 'dark' ? <Sun size={17} /> : <Moon size={17} />}
            </button>
            <button className="icon-button notification-button" aria-label="Notifications">
              <Bell size={17} />
              <span />
            </button>
            <div className="avatar">AB</div>
          </div>
        </header>

        <div className="content-wrap">
          <section className="page-heading">
            <div>
              <p className="eyebrow">{title.eyebrow}</p>
              <h1>{title.title}</h1>
              <p className="page-description">{title.description}</p>
            </div>
            {activeView !== 'settings' && (
              <div className="system-health">
                <div className="health-icon"><ShieldCheck size={20} /></div>
                <div>
                  <span>System health</span>
                  <strong>{healthLabel}</strong>
                </div>
                <ChevronDown size={16} />
              </div>
            )}
          </section>

          {activeView === 'overview' && <Overview metrics={metrics} energy={visor.snapshot?.energy} hardware={visor.snapshot?.hardware} processes={visor.processes} live={visor.connection === 'live'} onKill={visor.killProcess} onPriority={visor.setProcessPriority} />}
          {activeView === 'processes' && <ProcessExplorer metrics={metrics} processes={visor.processes} counts={visor.snapshot?.processCounts} live={visor.connection === 'live'} onKill={visor.killProcess} onPriority={visor.setProcessPriority} />}
          {activeView === 'performance' && <Performance metrics={metrics} />}
          {activeView === 'ai' && <AIWorkloads metrics={metrics} />}
          {activeView === 'energy' && <EnergyView energy={visor.snapshot?.energy} processes={visor.processes} agent={visor.snapshot?.agent} />}
          {activeView === 'history' && <HistoryView metrics={metrics} />}
          {activeView === 'alerts' && <AlertsView />}
          {activeView === 'settings' && <SettingsView theme={theme} setTheme={setTheme} />}
        </div>
      </main>

      {commandOpen && <CommandPalette onClose={() => setCommandOpen(false)} onNavigate={(view) => { setActiveView(view); setCommandOpen(false); }} />}
    </div>
  );
}

interface MetricsProps {
  metrics: ReturnType<typeof useVisorData>['metrics'];
}

interface ProcessActions {
  processes: ProcessInfo[];
  live: boolean;
  onKill: (pid: number) => Promise<unknown>;
  onPriority: (pid: number, priority: 'low' | 'belowNormal' | 'normal' | 'aboveNormal' | 'high') => Promise<unknown>;
}

interface OverviewProps extends MetricsProps, ProcessActions {
  energy?: EnergyEstimate;
  hardware?: HardwareInfo;
}

const bytesToGb = (bytes = 0) => bytes / 1024 ** 3;
const sensorValue = (value: number, suffix: string) => value > 0 ? `${Math.round(value)}${suffix}` : 'Unavailable';

function Overview({ metrics, energy, hardware, processes, live, onKill, onPriority }: OverviewProps) {
  const memoryUsed = bytesToGb(metrics.memory?.usedBytes);
  const memoryTotal = bytesToGb(metrics.memory?.totalBytes || hardware?.memory.totalBytes);
  const vramUsed = bytesToGb(metrics.gpuMemory?.usedBytes);
  const vramTotal = bytesToGb(metrics.gpuMemory?.totalBytes || hardware?.gpu?.vramBytes);
  return (
    <div className="dashboard-grid">
      <section className="hero-metrics">
        <MetricHero
          label="CPU"
          detail={hardware?.cpu.brand.trim() || 'Processor'}
          value={metrics.cpu}
          tone="cyan"
          history={metrics.history.cpu}
          stats={[
            ['Clock', metrics.cpuSpeedGhz ? `${metrics.cpuSpeedGhz.toFixed(2)} GHz` : 'Unavailable'],
            ['Temp', sensorValue(metrics.cpuTemp, '°C')],
            ['Power', `~${Math.round(metrics.cpuPower)} W`],
          ]}
        />
        <MetricHero
          label="GPU"
          detail={hardware?.gpu?.model || 'Graphics adapter'}
          value={metrics.gpu}
          tone="violet"
          history={metrics.history.gpu}
          stats={[
            ['VRAM', vramTotal ? `${vramTotal.toFixed(1)} GB` : 'Unavailable'],
            ['Temp', sensorValue(metrics.gpuTemp, '°C')],
            ['Power', `${Math.round(metrics.gpuPower)} W`],
          ]}
        />
      </section>

      <section className="compact-metrics">
        <CompactMetric
          label="Memory"
          value={metrics.ram}
          detail={memoryTotal ? `${memoryUsed.toFixed(1)} of ${memoryTotal.toFixed(1)} GB` : 'Memory sensor unavailable'}
          tone="mint"
          history={metrics.history.ram}
          icon={<MemoryStick size={18} />}
        />
        <CompactMetric
          label="VRAM"
          value={metrics.vram}
          detail={vramTotal ? `${vramUsed.toFixed(1)} of ${vramTotal.toFixed(1)} GB` : 'VRAM sensor unavailable'}
          tone="amber"
          history={metrics.history.vram}
          icon={<Gauge size={18} />}
        />
        <CompactMetric
          label="Disk"
          value={metrics.diskActivity || 0}
          detail={`${Math.round(metrics.diskRead)} MB/s read`}
          tone="rose"
          history={metrics.history.disk}
          icon={<HardDrive size={18} />}
        />
        <CompactMetric
          label="Network"
          value={Math.min(100, metrics.download * 1.5)}
          detail={`${metrics.download.toFixed(1)} Mbps down`}
          tone="cyan"
          history={metrics.history.network}
          icon={<Network size={18} />}
        />
      </section>

      <ProcessTable processes={processes} live={live} onKillProcess={onKill} onSetPriority={onPriority} />

      <section className="insight-row">
        <AIInsight processes={processes} />
        <EfficiencyCard metrics={metrics} energy={energy} />
        <ThermalCard metrics={metrics} />
      </section>
    </div>
  );
}

interface MetricHeroProps {
  label: string;
  detail: string;
  value: number;
  tone: MetricTone;
  history: number[];
  stats: [string, string][];
}

function MetricHero({ label, detail, value, tone, history, stats }: MetricHeroProps) {
  return (
    <article className={`panel metric-hero tone-${tone}`}>
      <div className="metric-hero-top">
        <div>
          <div className="metric-label-line"><span className="metric-dot" />{label}</div>
          <p>{detail}</p>
        </div>
        <button className="expand-button" aria-label={`Expand ${label}`}><Maximize2 size={15} /></button>
      </div>
      <div className="metric-hero-body">
        <Donut value={value} tone={tone} label={label} size="large" />
        <div className="hero-chart">
          <div className="hero-chart-labels"><span>60 SEC</span><span>NOW</span></div>
          <LineChart values={history} tone={tone} height={122} />
        </div>
      </div>
      <div className="metric-stats">
        {stats.map(([name, stat]) => <div key={name}><span>{name}</span><strong>{stat}</strong></div>)}
      </div>
    </article>
  );
}

interface CompactMetricProps {
  label: string;
  value: number;
  detail: string;
  tone: MetricTone;
  history: number[];
  icon: React.ReactNode;
}

function CompactMetric({ label, value, detail, tone, history, icon }: CompactMetricProps) {
  return (
    <article className={`panel compact-card tone-${tone}`}>
      <div className="compact-card-head"><span>{icon}</span><strong>{label}</strong><em>{Math.round(value)}%</em></div>
      <div className="compact-chart"><LineChart values={history} tone={tone} height={62} compact /></div>
      <p>{detail}</p>
    </article>
  );
}

function AIInsight({ processes }: { processes: ProcessInfo[] }) {
  const workload = processes.find((process) => process.kind === 'AI');
  return (
    <article className="panel insight-card ai-insight">
      <div className="insight-card-head">
        <div className="insight-icon violet-bg"><Bot size={19} /></div>
        <div><span>AI WORKLOAD</span><strong>{workload ? 'Detected locally' : 'No active model'}</strong></div>
        {workload && <span className="status-badge"><i /> RUNNING</span>}
      </div>
      <div className="model-name"><Sparkles size={16} /><strong>{workload?.name || 'Waiting for an AI runtime'}</strong><span>{workload ? `PID ${workload.id}` : 'LOCAL'}</span></div>
      <div className="model-stats">
        <div><span>Attributed power</span><strong>~{(workload?.energyWatts || 0).toFixed(1)} <small>W</small></strong></div>
        <div><span>GPU load</span><strong>{(workload?.gpu || 0).toFixed(1)}%</strong></div>
        <div><span>VRAM</span><strong>{(workload?.vram || 0).toFixed(1)} <small>GB</small></strong></div>
      </div>
    </article>
  );
}

function EfficiencyCard({ metrics, energy }: MetricsProps & { energy?: EnergyEstimate }) {
  const score = Math.max(40, Math.round(100 - (energy?.watts || metrics.cpuPower + metrics.gpuPower) / 6));
  const rating = score >= 80 ? 'Excellent' : score >= 60 ? 'Balanced' : 'High draw';
  return (
    <article className="panel insight-card efficiency-card">
      <div className="insight-card-head">
        <div className="insight-icon mint-bg"><Zap size={19} /></div>
        <div><span>ENERGY LENS</span><strong>{energy ? `${energy.watts.toFixed(0)} W at the wall` : 'Power estimate'}</strong></div>
      </div>
      <div className="score-row"><strong>{score}</strong><span>/ 100</span><em>{rating}</em></div>
      <div className="score-track"><span style={{ width: `${score}%` }} /></div>
      <p>{energy ? `${energy.confidenceScore}% confidence · ${energy.sessionWh.toFixed(2)} Wh this session` : 'Start the Windows agent for a real-time wall-power estimate.'}</p>
    </article>
  );
}

function ThermalCard({ metrics }: MetricsProps) {
  const sensedTemperatures = [metrics.cpuTemp, metrics.gpuTemp].filter((value) => value > 0);
  const hottestTemperature = sensedTemperatures.length ? Math.max(...sensedTemperatures) : 0;
  const thermalStatus = !hottestTemperature
    ? 'Sensors unavailable'
    : hottestTemperature >= 90
      ? 'Thermal alert'
      : hottestTemperature >= 80
        ? 'Running warm'
        : 'Within range';
  return (
    <article className="panel insight-card thermal-card">
      <div className="insight-card-head">
        <div className="insight-icon amber-bg"><Thermometer size={19} /></div>
        <div><span>THERMALS</span><strong>{thermalStatus}</strong></div>
      </div>
      <div className="thermal-readings">
        <div><span>CPU</span><strong>{metrics.cpuTemp > 0 ? `${Math.round(metrics.cpuTemp)}°` : '—'}</strong><i style={{ width: `${metrics.cpuTemp}%` }} /></div>
        <div><span>GPU</span><strong>{metrics.gpuTemp > 0 ? `${Math.round(metrics.gpuTemp)}°` : '—'}</strong><i style={{ width: `${metrics.gpuTemp}%` }} /></div>
        <div><span>NVMe</span><strong>—</strong><i style={{ width: '0%' }} /></div>
      </div>
    </article>
  );
}

function ProcessExplorer({ metrics, processes, counts, live, onKill, onPriority }: MetricsProps & ProcessActions & { counts?: { all: number; running: number } }) {
  const totalThreads = processes.reduce((sum, process) => sum + (process.threads || 0), 0);
  const powerHungry = processes.filter((process) => (process.energyWatts || 0) >= 10);
  return (
    <div className="single-page-grid">
      <section className="summary-strip">
        <SummaryItem icon={<Activity />} label="Processes" value={String(counts?.all || processes.length)} detail={`${totalThreads.toLocaleString()} threads`} />
        <SummaryItem icon={<Cpu />} label="CPU load" value={`${Math.round(metrics.cpu)}%`} detail={`${metrics.cpuSpeedGhz?.toFixed(2) || '—'} GHz average`} />
        <SummaryItem icon={<MemoryStick />} label="Memory" value={`${bytesToGb(metrics.memory?.usedBytes).toFixed(1)} GB`} detail={`${bytesToGb(metrics.memory?.availableBytes).toFixed(1)} GB available`} />
        <SummaryItem icon={<Zap />} label="Power hungry" value={String(powerHungry.length)} detail={powerHungry[0]?.name || 'None detected'} warning />
      </section>
      <ProcessTable expanded processes={processes} live={live} onKillProcess={onKill} onSetPriority={onPriority} />
    </div>
  );
}

function SummaryItem({ icon, label, value, detail, warning = false }: { icon: React.ReactNode; label: string; value: string; detail: string; warning?: boolean }) {
  return <article className="panel summary-item"><span className={warning ? 'summary-warning' : ''}>{icon}</span><div><p>{label}</p><strong>{value}</strong><small>{detail}</small></div></article>;
}

function Performance({ metrics }: MetricsProps) {
  const cards: Array<{ key: MetricKey | 'network' | 'disk'; label: string; value: string; detail: string; tone: MetricTone; icon: React.ReactNode }> = [
    { key: 'cpu', label: 'Processor', value: `${Math.round(metrics.cpu)}%`, detail: '5.2 GHz · 16 cores', tone: 'cyan', icon: <Cpu /> },
    { key: 'gpu', label: 'Graphics', value: `${Math.round(metrics.gpu)}%`, detail: `${Math.round(metrics.gpuTemp)}°C · ${Math.round(metrics.gpuPower)} W`, tone: 'violet', icon: <Gauge /> },
    { key: 'ram', label: 'Memory', value: `${Math.round(metrics.ram)}%`, detail: '21.8 / 32 GB', tone: 'mint', icon: <MemoryStick /> },
    { key: 'vram', label: 'Video memory', value: `${Math.round(metrics.vram)}%`, detail: '18.2 / 24 GB', tone: 'amber', icon: <CircleGauge /> },
    { key: 'disk', label: 'NVMe drive', value: `${Math.round(metrics.diskRead)} MB/s`, detail: `${Math.round(metrics.diskWrite)} MB/s write`, tone: 'rose', icon: <HardDrive /> },
    { key: 'network', label: 'Ethernet', value: `${metrics.download.toFixed(1)} Mbps`, detail: `${metrics.upload.toFixed(1)} Mbps upload`, tone: 'cyan', icon: <Network /> },
  ];
  return (
    <div className="performance-grid">
      {cards.map((card) => (
        <article key={card.label} className={`panel performance-card tone-${card.tone}`}>
          <div className="performance-card-head"><span>{card.icon}</span><div><p>{card.label}</p><strong>{card.value}</strong><small>{card.detail}</small></div><button className="expand-button"><Maximize2 size={15} /></button></div>
          <div className="performance-chart"><LineChart values={metrics.history[card.key]} tone={card.tone} height={156} /></div>
          <div className="chart-axis"><span>60 seconds ago</span><span>Now</span></div>
        </article>
      ))}
    </div>
  );
}

function AIWorkloads({ metrics }: MetricsProps) {
  return (
    <div className="ai-page-grid">
      <section className="panel ai-model-card">
        <div className="ai-model-hero">
          <div className="model-orb"><Bot size={28} /><span /></div>
          <div><p className="eyebrow">OLLAMA · ACTIVE NOW</p><h2>Llama 3.3 70B</h2><span>Q4_K_M · 42.5 GB · CUDA</span></div>
          <span className="status-badge"><i /> INFERENCE</span>
        </div>
        <div className="ai-primary-stats">
          <div><span>Inference speed</span><strong>42.8</strong><small>tokens / sec</small></div>
          <div><span>Context used</span><strong>18.4</strong><small>k / 128k</small></div>
          <div><span>Time to first token</span><strong>184</strong><small>milliseconds</small></div>
        </div>
        <div className="inference-chart-head"><div><strong>Inference activity</strong><span>Tokens generated over the last minute</span></div><button className="ghost-button">Last minute <ChevronDown size={14} /></button></div>
        <div className="inference-chart"><LineChart values={metrics.history.gpu.map((value) => value * 0.86)} tone="violet" height={180} /></div>
      </section>
      <aside className="ai-side-column">
        <article className="panel allocation-card"><p className="eyebrow">RESOURCE ALLOCATION</p><h3>Model footprint</h3><ResourceBar label="GPU" value={42} detail="42%" tone="violet" /><ResourceBar label="VRAM" value={59} detail="14.2 / 24 GB" tone="amber" /><ResourceBar label="RAM" value={27} detail="8.5 / 32 GB" tone="mint" /><ResourceBar label="CPU" value={18} detail="18%" tone="cyan" /></article>
        <article className="panel runtime-card"><p className="eyebrow">RUNTIME DETAILS</p><dl><div><dt>Process</dt><dd>ollama.exe</dd></div><div><dt>PID</dt><dd>14280</dd></div><div><dt>Backend</dt><dd>CUDA 13.0</dd></div><div><dt>GPU layers</dt><dd>81 / 81</dd></div><div><dt>Uptime</dt><dd>01:42:18</dd></div></dl></article>
      </aside>
    </div>
  );
}

function ResourceBar({ label, value, detail, tone }: { label: string; value: number; detail: string; tone: MetricTone }) {
  return <div className={`resource-bar tone-${tone}`}><div><span>{label}</span><strong>{detail}</strong></div><i><b style={{ width: `${value}%` }} /></i></div>;
}

function EnergyView({ energy, processes, agent }: { energy?: EnergyEstimate; processes: ProcessInfo[]; agent?: AgentInfo }) {
  if (!energy) {
    return <section className="panel energy-offline"><div className="insight-icon amber-bg"><Zap size={21} /></div><div><p className="eyebrow">WINDOWS AGENT REQUIRED</p><h2>Energy Lens is waiting for real sensors.</h2><span>Run `npm.cmd run agent` locally to enable power measurement and process attribution.</span></div></section>;
  }

  const attributed = [...processes].sort((a, b) => (b.energyWatts || 0) - (a.energyWatts || 0)).slice(0, 7);
  const breakdown = [
    { label: 'Processor', value: energy.breakdown.cpu, tone: 'cyan' as const },
    { label: 'Graphics', value: energy.breakdown.gpu, tone: 'violet' as const },
    { label: 'Memory', value: energy.breakdown.memory, tone: 'mint' as const },
    { label: 'Storage', value: energy.breakdown.storage, tone: 'rose' as const },
    { label: 'Platform + conversion', value: energy.breakdown.platform + energy.breakdown.conversionLoss, tone: 'amber' as const },
  ];
  const maxBreakdown = Math.max(...breakdown.map((item) => item.value), 1);

  return (
    <div className="energy-page-grid">
      <section className="panel energy-live-card">
        <div className="energy-live-head"><div className="insight-icon mint-bg"><Leaf size={20} /></div><div><p className="eyebrow">LIVE WALL-POWER ESTIMATE</p><h2>Energy Lens</h2></div><span className={`confidence-badge confidence-${energy.confidence}`}>{energy.confidenceScore}% confidence</span></div>
        <div className="energy-live-value"><strong>{energy.watts.toFixed(0)}</strong><span>W</span><em>right now</em></div>
        <div className="energy-session-line"><span>Session consumption</span><strong>{energy.sessionWh.toFixed(2)} Wh</strong><i /><span>At the current load</span><strong>{energy.dailyKwh.toFixed(2)} kWh/day</strong></div>
        <p>{energy.methodology}</p>
      </section>

      <section className="panel energy-breakdown-card">
        <div className="panel-header"><div><p className="eyebrow">POWER PATH</p><h2>Where each watt goes</h2></div><span>Live estimate</span></div>
        <div className="energy-breakdown-list">{breakdown.map((item) => <div className={`energy-breakdown-row tone-${item.tone}`} key={item.label}><span>{item.label}</span><i><b style={{ width: `${item.value / maxBreakdown * 100}%` }} /></i><strong>{item.value.toFixed(1)} W</strong></div>)}</div>
      </section>

      <section className="energy-projection-grid">
        <SummaryItem icon={<Clock3 />} label="Cost at current load" value={`€${energy.dailyCost.toFixed(2)}`} detail={`per day · €${energy.tariffPerKwh.toFixed(2)}/kWh`} />
        <SummaryItem icon={<Zap />} label="30-day projection" value={`€${energy.monthlyCost.toFixed(2)}`} detail={`${(energy.dailyKwh * 30).toFixed(1)} kWh`} />
        <SummaryItem icon={<Leaf />} label="Carbon projection" value={`${energy.dailyCarbonGrams} g`} detail={`CO₂e/day · ${energy.carbonGramsPerKwh} g/kWh`} />
        <SummaryItem icon={<Activity />} label="VISOR overhead" value={`${agent?.cpuPercent.toFixed(2) || '—'}%`} detail={`${agent?.memoryMb.toFixed(0) || '—'} MB RAM`} />
      </section>

      <section className="panel energy-attribution-card">
        <div className="panel-header"><div><p className="eyebrow">ENERGY FINGERPRINTS</p><h2>Applications responsible right now</h2></div><span>Modeled attribution</span></div>
        <div className="energy-process-list">{attributed.map((process) => {
          const dailySavings = (process.energyWatts || 0) * 24 / 1000 * energy.tariffPerKwh;
          return <div className="energy-process-row" key={process.id}><span className="app-icon" style={{ background: process.color }}>{process.icon}</span><div><strong>{process.name}</strong><span>PID {process.id} · CPU {process.cpu.toFixed(1)}% · GPU {process.gpu.toFixed(1)}%</span></div><div className="energy-process-impact"><strong>~{(process.energyWatts || 0).toFixed(1)} W</strong><span>€{dailySavings.toFixed(2)}/day at this load</span></div></div>;
        })}</div>
      </section>

      <section className="panel energy-trust-card"><ShieldCheck size={19} /><div><strong>No fake precision</strong><span>GPU power is measured through vendor telemetry when available. CPU, platform losses, and per-process energy are modeled and labeled. Tariff and carbon factors are assumptions you can change.</span></div></section>
    </div>
  );
}

function HistoryView({ metrics }: MetricsProps) {
  const [range, setRange] = useState('1 hour');
  const history = useMemo(() => Array.from({ length: 64 }, (_, index) => Math.min(96, Math.max(8, metrics.history.cpu[index % metrics.history.cpu.length] + Math.sin(index / 5) * 15))), [metrics.history.cpu]);
  return (
    <div className="history-grid">
      <section className="panel history-chart-card">
        <div className="panel-header"><div><p className="eyebrow">SYSTEM LOAD</p><h2>Performance timeline</h2></div><div className="segmented-control">{['1 hour', '24 hours', '7 days'].map((item) => <button key={item} className={range === item ? 'active' : ''} onClick={() => setRange(item)}>{item}</button>)}</div></div>
        <div className="history-legend"><span><i className="cyan-legend" />CPU average 44%</span><span><i className="violet-legend" />GPU average 68%</span></div>
        <div className="history-main-chart"><LineChart values={history} tone="cyan" height={250} /></div>
        <div className="history-axis"><span>10:00</span><span>10:15</span><span>10:30</span><span>10:45</span><span>Now</span></div>
      </section>
      <section className="peak-grid">
        <PeakCard label="Peak CPU" value="92%" time="10:24" icon={<Cpu />} />
        <PeakCard label="Peak GPU" value="98%" time="10:37" icon={<Gauge />} />
        <PeakCard label="Peak memory" value="24.6 GB" time="10:41" icon={<MemoryStick />} />
        <PeakCard label="Energy used" value="0.42 kWh" time="This session" icon={<Zap />} />
      </section>
      <section className="panel session-card"><div><div className="session-icon"><Clock3 /></div><div><p className="eyebrow">CURRENT SESSION</p><h3>Started today at 08:42</h3><span>2h 14m recorded · 2 notable peaks</span></div></div><button className="ghost-button"><Download size={15} /> Export data</button></section>
    </div>
  );
}

function PeakCard({ label, value, time, icon }: { label: string; value: string; time: string; icon: React.ReactNode }) {
  return <article className="panel peak-card"><span>{icon}</span><div><p>{label}</p><strong>{value}</strong><small>{time}</small></div></article>;
}

function AlertsView() {
  const [rules, setRules] = useState([
    { id: 1, title: 'GPU temperature', condition: 'Above 80°C for 30 seconds', enabled: true, icon: <Thermometer />, tone: 'amber' },
    { id: 2, title: 'VRAM pressure', condition: 'Above 95% for 10 seconds', enabled: true, icon: <Gauge />, tone: 'violet' },
    { id: 3, title: 'CPU sustained load', condition: 'Above 90% for 2 minutes', enabled: false, icon: <Cpu />, tone: 'cyan' },
    { id: 4, title: 'Network anomaly', condition: 'Unusual upload activity detected', enabled: true, icon: <Network />, tone: 'rose' },
  ]);
  return (
    <div className="alerts-grid">
      <section className="panel alert-summary"><div className="alert-summary-icon"><Check /></div><div><p className="eyebrow">ALL CLEAR</p><h2>No active alerts</h2><span>Your hardware is operating within the limits you set.</span></div><button className="primary-button"><Plus size={16} /> New alert</button></section>
      <section className="panel rules-card"><div className="panel-header"><div><p className="eyebrow">AUTOMATIONS</p><h2>Alert rules</h2></div><span>{rules.filter((rule) => rule.enabled).length} active</span></div><div className="rules-list">{rules.map((rule) => <div className="rule-row" key={rule.id}><span className={`rule-icon rule-${rule.tone}`}>{rule.icon}</span><div><strong>{rule.title}</strong><span>{rule.condition}</span></div><button className={`switch ${rule.enabled ? 'switch-on' : ''}`} onClick={() => setRules((current) => current.map((item) => item.id === rule.id ? { ...item, enabled: !item.enabled } : item))}><span /></button></div>)}</div></section>
      <section className="panel recent-alert"><div className="recent-alert-head"><span><Info size={16} /></span><div><strong>VRAM pressure reached 91%</strong><small>Yesterday · 21:48</small></div><em>Resolved</em></div><p>Cyberpunk 2077 used 13.8 GB while Ollama held 8.2 GB. VISOR released the alert when usage returned below 85%.</p></section>
    </div>
  );
}

function SettingsView({ theme, setTheme }: { theme: 'dark' | 'light'; setTheme: (theme: 'dark' | 'light') => void }) {
  const [rate, setRate] = useState('900 ms');
  const [toggles, setToggles] = useState({ startup: true, notifications: true, minimize: true, anonymous: false });
  const toggle = (key: keyof typeof toggles) => setToggles((values) => ({ ...values, [key]: !values[key] }));
  return (
    <div className="settings-grid">
      <section className="panel settings-card"><div className="settings-heading"><span><Sun /></span><div><h2>Appearance</h2><p>A calmer monitor is an easier monitor.</p></div></div><div className="theme-choices"><button className={theme === 'dark' ? 'active' : ''} onClick={() => setTheme('dark')}><span className="theme-preview theme-dark"><i /><b /><em /></span><strong><Moon size={15} /> Dark</strong></button><button className={theme === 'light' ? 'active' : ''} onClick={() => setTheme('light')}><span className="theme-preview theme-light"><i /><b /><em /></span><strong><Sun size={15} /> Light</strong></button></div></section>
      <section className="panel settings-card"><div className="settings-heading"><span><Activity /></span><div><h2>Monitoring</h2><p>Balance responsiveness and resource use.</p></div></div><div className="setting-row"><div><strong>Refresh interval</strong><span>How often VISOR samples your hardware</span></div><div className="segmented-control">{['500 ms', '900 ms', '2 sec'].map((item) => <button key={item} className={rate === item ? 'active' : ''} onClick={() => setRate(item)}>{item}</button>)}</div></div><SettingToggle label="Start with Windows" detail="Open quietly in the system tray" enabled={toggles.startup} onClick={() => toggle('startup')} /><SettingToggle label="Minimize to tray" detail="Keep monitoring when the window closes" enabled={toggles.minimize} onClick={() => toggle('minimize')} /></section>
      <section className="panel settings-card"><div className="settings-heading"><span><Bell /></span><div><h2>Notifications</h2><p>Useful signals, never noise.</p></div></div><SettingToggle label="Windows notifications" detail="Show critical alerts outside VISOR" enabled={toggles.notifications} onClick={() => toggle('notifications')} /><SettingToggle label="Anonymous diagnostics" detail="Help improve stability without sending metrics" enabled={toggles.anonymous} onClick={() => toggle('anonymous')} /></section>
    </div>
  );
}

function SettingToggle({ label, detail, enabled, onClick }: { label: string; detail: string; enabled: boolean; onClick: () => void }) {
  return <div className="setting-row"><div><strong>{label}</strong><span>{detail}</span></div><button className={`switch ${enabled ? 'switch-on' : ''}`} onClick={onClick}><span /></button></div>;
}

function CommandPalette({ onClose, onNavigate }: { onClose: () => void; onNavigate: (view: ViewId) => void }) {
  return (
    <div className="command-backdrop" onMouseDown={onClose}>
      <div className="command-palette" onMouseDown={(event) => event.stopPropagation()}>
        <div className="command-search"><Search size={18} /><input autoFocus placeholder="Search VISOR or type a command…" /><button onClick={onClose}><X size={16} /></button></div>
        <p>QUICK NAVIGATION</p>
        <button onClick={() => onNavigate('processes')}><span><Cpu size={18} /> View all processes</span><Command size={14} /></button>
        <button onClick={() => onNavigate('ai')}><span><Bot size={18} /> Open AI workloads</span><em>1 active</em></button>
        <button onClick={() => onNavigate('performance')}><span><Gauge size={18} /> Inspect GPU performance</span><em>78%</em></button>
        <button onClick={() => onNavigate('alerts')}><span><Bell size={18} /> Configure smart alerts</span></button>
        <div className="command-footer"><span><kbd>↑↓</kbd> Navigate</span><span><kbd>Enter</kbd> Select</span><span><kbd>Esc</kbd> Close</span></div>
      </div>
    </div>
  );
}

export default App;
