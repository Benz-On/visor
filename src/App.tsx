import { useEffect, useState } from 'react';
import {
  Activity,
  Bell,
  Bot,
  Cloud,
  Check,
  ChevronDown,
  CircleGauge,
  Clock3,
  Command,
  Cpu,
  Gauge,
  Gamepad2,
  HardDrive,
  Info,
  Leaf,
  Maximize2,
  MemoryStick,
  Network,
  Palette,
  Pause,
  Play,
  Search,
  Server,
  ShieldCheck,
  ShieldAlert,
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
import { analyzeLocalModel } from './modelAdvisor';
import type { AgentInfo, AlertsSnapshot, EnergyEstimate, HardwareInfo, LocalAiSnapshot, LocalModelInfo, MetricKey, MetricTone, ProcessInfo, ThemeId } from './types';

const themeOrder: ThemeId[] = ['studio', 'porcelain', 'cyber', 'retro'];
const themeNames: Record<ThemeId, string> = {
  studio: 'Studio',
  porcelain: 'Porcelain',
  cyber: 'Cyberdeck',
  retro: 'Retro terminal',
};

const viewTitles: Record<ViewId, { eyebrow: string; title: string; description: string }> = {
  overview: {
    eyebrow: 'SYSTEM OVERVIEW',
    title: 'Your system, clearly understood.',
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
    description: 'Exact application, runtime, model identity and live resource footprint.',
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
  const [theme, setTheme] = useState<ThemeId>(() => {
    const saved = window.localStorage.getItem('visor-theme');
    return themeOrder.includes(saved as ThemeId) ? saved as ThemeId : 'studio';
  });
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

  useEffect(() => {
    window.localStorage.setItem('visor-theme', theme);
  }, [theme]);

  const title = viewTitles[activeView];
  const thermalAlert = metrics.cpuTemp >= 90 || metrics.gpuTemp >= 86;
  const activeAlertCount = visor.snapshot?.alerts?.active.length || 0;
  const healthLabel = visor.connection === 'demo'
    ? 'Demo mode'
    : visor.connection === 'error'
      ? 'Reconnecting'
      : visor.connection === 'connecting'
        ? 'Connecting'
        : activeAlertCount > 0 || thermalAlert
          ? 'Attention'
          : 'Telemetry active';
  const cycleTheme = () => setTheme((current) => themeOrder[(themeOrder.indexOf(current) + 1) % themeOrder.length]);

  return (
    <div className="app-shell" data-theme={theme}>
      <Sidebar
        active={activeView}
        collapsed={collapsed}
        onNavigate={setActiveView}
        onCollapse={() => setCollapsed((value) => !value)}
        connection={visor.connection}
        activeModelCount={visor.snapshot?.localAI?.loadedModelCount || 0}
        activeAlertCount={activeAlertCount}
        collectorSource={visor.snapshot?.source}
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
            <button className="icon-button theme-cycle" onClick={cycleTheme} aria-label={`Change theme. Current: ${themeNames[theme]}`} title={`Theme: ${themeNames[theme]}`}>
              <Palette size={17} />
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

          {activeView === 'overview' && <Overview metrics={metrics} energy={visor.snapshot?.energy} hardware={visor.snapshot?.hardware} localAI={visor.snapshot?.localAI} processes={visor.processes} live={visor.connection === 'live'} onKill={visor.killProcess} onPriority={visor.setProcessPriority} />}
          {activeView === 'processes' && <ProcessExplorer metrics={metrics} processes={visor.processes} counts={visor.snapshot?.processCounts} live={visor.connection === 'live'} onKill={visor.killProcess} onPriority={visor.setProcessPriority} />}
          {activeView === 'performance' && <Performance metrics={metrics} hardware={visor.snapshot?.hardware} />}
          {activeView === 'ai' && <AIWorkloads metrics={metrics} localAI={visor.snapshot?.localAI} hardware={visor.snapshot?.hardware} />}
          {activeView === 'energy' && <EnergyView energy={visor.snapshot?.energy} processes={visor.processes} agent={visor.snapshot?.agent} />}
          {activeView === 'history' && <HistoryView metrics={metrics} energy={visor.snapshot?.energy} agent={visor.snapshot?.agent} />}
          {activeView === 'alerts' && <AlertsView alerts={visor.snapshot?.alerts} live={visor.connection === 'live'} onToggle={visor.setAlertRule} />}
          {activeView === 'settings' && <SettingsView theme={theme} setTheme={setTheme} metrics={metrics} hardware={visor.snapshot?.hardware} agent={visor.snapshot?.agent} source={visor.snapshot?.source} />}
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
  localAI?: LocalAiSnapshot;
}

const bytesToGb = (bytes = 0) => bytes / 1024 ** 3;
const sensorValue = (value: number, suffix: string) => value > 0 ? `${Math.round(value)}${suffix}` : 'Unavailable';

function Overview({ metrics, energy, hardware, localAI, processes, live, onKill, onPriority }: OverviewProps) {
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
        <AIInsight processes={processes} localAI={localAI} />
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
  const loadState = value >= 90 ? 'metric-critical' : value >= 75 ? 'metric-elevated' : '';
  return (
    <article className={`panel metric-hero tone-${tone} ${loadState}`}>
      <div className="metric-hero-top">
        <div>
          <div className="metric-label-line"><span className="metric-dot" />{label}{loadState && <em>{value >= 90 ? 'CRITICAL LOAD' : 'HIGH LOAD'}</em>}</div>
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
  const loadState = value >= 90 ? 'metric-critical' : value >= 75 ? 'metric-elevated' : '';
  return (
    <article className={`panel compact-card tone-${tone} ${loadState}`}>
      <div className="compact-card-head"><span>{icon}</span><strong>{label}</strong><em>{Math.round(value)}%</em></div>
      <div className="compact-chart"><LineChart values={history} tone={tone} height={62} compact /></div>
      <p>{detail}</p>
    </article>
  );
}

function AIInsight({ processes, localAI }: { processes: ProcessInfo[]; localAI?: LocalAiSnapshot }) {
  const workload = processes.find((process) => process.kind === 'AI');
  const model = localAI?.models.find((item) => item.status === 'active') || localAI?.models[0];
  const application = localAI?.applications.find((item) => item.application === model?.application) || (!model ? localAI?.applications[0] : undefined);
  const modelDetected = Boolean(model || application || workload);
  return (
    <article className="panel insight-card ai-insight">
      <div className="insight-card-head">
        <div className="insight-icon violet-bg"><Bot size={19} /></div>
        <div><span>LOCAL AI</span><strong>{model ? `${model.application} · ${model.runtime}` : application ? `${application.application} detected` : 'No active model'}</strong></div>
        {modelDetected && <span className="status-badge"><i /> {model?.status === 'active' ? 'ACTIVE' : model?.status === 'loaded' ? 'LOADED' : 'INSTALLED'}</span>}
      </div>
      <div className="model-name"><Sparkles size={16} /><strong>{model?.model || 'Waiting for a runtime API'}</strong><span>{model ? `${model.confidence}% exact` : workload ? `PID ${workload.id}` : 'LOCAL'}</span></div>
      <div className="model-stats">
        <div><span>Application power</span><strong>~{(model?.applicationEnergyWatts ?? application?.energyWatts ?? workload?.energyWatts ?? 0).toFixed(1)} <small>W</small></strong></div>
        <div><span>GPU activity</span><strong>{(model?.applicationGpu ?? application?.gpu ?? workload?.gpu ?? 0).toFixed(1)}%</strong></div>
        <div><span>Model VRAM</span><strong>{(model?.allocatedVramGb ?? application?.vramGb ?? workload?.vram ?? 0).toFixed(1)} <small>GB</small></strong></div>
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
      <p>{energy ? `${energy.confidenceScore}% confidence · ${energy.sessionWh.toFixed(2)} Wh this session` : 'Start the native collector or development agent for a real-time power estimate.'}</p>
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
  const thermalReadings = [
    { label: 'CPU', value: metrics.cpuTemp, source: metrics.sensorSources?.cpu },
    { label: 'GPU', value: metrics.gpuTemp, source: metrics.sensorSources?.gpu },
    { label: 'SSD', value: metrics.ssdTemp || 0, source: metrics.storageTemperatures?.[0]?.name },
  ];
  return (
    <article className="panel insight-card thermal-card">
      <div className="insight-card-head">
        <div className="insight-icon amber-bg"><Thermometer size={19} /></div>
        <div><span>THERMALS</span><strong>{thermalStatus}</strong></div>
      </div>
      <div className="thermal-readings">
        {thermalReadings.map((reading) => <div className={`thermal-sensor ${reading.value <= 0 ? 'sensor-unavailable' : reading.value >= 86 ? 'sensor-hot' : ''}`} key={reading.label}><span>{reading.label}<small>{reading.source || 'Sensor unavailable'}</small></span><strong>{reading.value > 0 ? `${Math.round(reading.value)}°C` : '—'}</strong><i><b style={{ width: `${Math.min(100, reading.value)}%` }} /></i></div>)}
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

function Performance({ metrics, hardware }: MetricsProps & { hardware?: HardwareInfo }) {
  const memoryUsed = bytesToGb(metrics.memory?.usedBytes);
  const memoryTotal = bytesToGb(metrics.memory?.totalBytes || hardware?.memory.totalBytes);
  const vramUsed = bytesToGb(metrics.gpuMemory?.usedBytes);
  const vramTotal = bytesToGb(metrics.gpuMemory?.totalBytes || hardware?.gpu?.vramBytes);
  const coreCount = hardware?.cpu.physicalCores || hardware?.cpu.cores;
  const cards: Array<{ key: MetricKey | 'network' | 'disk'; label: string; value: string; detail: string; tone: MetricTone; icon: React.ReactNode }> = [
    { key: 'cpu', label: hardware?.cpu.brand.trim() || 'Processor', value: `${Math.round(metrics.cpu)}%`, detail: `${metrics.cpuSpeedGhz ? `${metrics.cpuSpeedGhz.toFixed(2)} GHz` : 'Clock unavailable'} · ${coreCount ? `${coreCount} cores` : 'Core count unavailable'}`, tone: 'cyan', icon: <Cpu /> },
    { key: 'gpu', label: hardware?.gpu?.model || 'Graphics', value: `${Math.round(metrics.gpu)}%`, detail: `${sensorValue(metrics.gpuTemp, '°C')} · ${Math.round(metrics.gpuPower)} W`, tone: 'violet', icon: <Gauge /> },
    { key: 'ram', label: 'Memory', value: `${Math.round(metrics.ram)}%`, detail: memoryTotal ? `${memoryUsed.toFixed(1)} / ${memoryTotal.toFixed(1)} GB` : 'Capacity unavailable', tone: 'mint', icon: <MemoryStick /> },
    { key: 'vram', label: 'Video memory', value: `${Math.round(metrics.vram)}%`, detail: vramTotal ? `${vramUsed.toFixed(1)} / ${vramTotal.toFixed(1)} GB` : 'Capacity unavailable', tone: 'amber', icon: <CircleGauge /> },
    { key: 'disk', label: hardware?.storage[0]?.name || 'Storage', value: `${metrics.diskRead.toFixed(1)} MB/s`, detail: `${metrics.diskWrite.toFixed(1)} MB/s write`, tone: 'rose', icon: <HardDrive /> },
    { key: 'network', label: 'Network', value: `${metrics.download.toFixed(1)} Mbps`, detail: `${metrics.upload.toFixed(1)} Mbps upload`, tone: 'cyan', icon: <Network /> },
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

const modelSizeParts = (model: LocalModelInfo) => {
  const bytes = model.sizeBytes || model.allocatedBytes || 0;
  if (!bytes) return { value: '—', unit: 'size unavailable' };
  return bytes >= 1024 ** 3
    ? { value: bytesToGb(bytes).toFixed(bytesToGb(bytes) >= 10 ? 1 : 2), unit: 'GB' }
    : { value: String(Math.round(bytes / 1024 ** 2)), unit: 'MB' };
};

const modelSize = (model: LocalModelInfo) => Object.values(modelSizeParts(model)).join(' ');

const tokenSpeed = (minimum: number | null, maximum: number | null, workload: 'generation' | 'embedding' = 'generation') => workload === 'embedding'
  ? 'Embedding model'
  : minimum !== null && maximum !== null ? `${minimum}–${maximum} tok/s` : 'Speed unknown';

function AIWorkloads({ metrics, localAI, hardware }: MetricsProps & { localAI?: LocalAiSnapshot; hardware?: HardwareInfo }) {
  const preferredModel = localAI?.models.find((item) => item.status === 'active') || localAI?.models.find((item) => item.status === 'loaded') || localAI?.models[0];
  const [selectedModelId, setSelectedModelId] = useState('');
  const model = localAI?.models.find((item) => item.id === selectedModelId) || preferredModel;
  const application = localAI?.applications.find((item) => item.application === model?.application) || (!model ? localAI?.applications[0] : undefined);
  const runtimeProcess = model?.process || application?.processes.find((item) => item.role === 'model-runner' || item.role === 'runtime') || application?.processes[0];
  const allocatedRamGb = bytesToGb(model?.allocatedRamBytes);
  const modelVramGb = model?.allocatedVramGb || (model?.status !== 'detected' ? application?.vramGb : 0) || 0;
  const totalVramGb = bytesToGb(metrics.gpuMemory?.totalBytes);
  const appEnergy = model?.applicationEnergyWatts ?? application?.energyWatts ?? 0;
  const exactModel = Boolean(model);
  const compatibility = model ? analyzeLocalModel(model, hardware) : null;
  const selectedModelSize = model ? modelSizeParts(model) : null;
  const installedModels = localAI?.models || [];
  const services = localAI?.applications || [];
  return (
    <div className="ai-page-grid">
      <section className="panel ai-model-card">
        <div className="ai-model-hero">
          <div className="model-orb"><Bot size={28} /><span /></div>
          <div><p className="eyebrow">{model ? `${model.application.toUpperCase()} · ${model.status.toUpperCase()}` : 'LOCAL RUNTIME DETECTION'}</p><h2>{model?.model || application?.application || 'No local model loaded'}</h2><span>{model ? [model.parameters, model.quantization, model.format?.toUpperCase()].filter(Boolean).join(' · ') : 'VISOR is watching local runtime APIs and process trees.'}</span></div>
          <span className={`status-badge ${exactModel ? '' : 'status-neutral'}`}><i /> {model ? `${model.confidence}% EXACT` : application ? 'PROCESS ONLY' : 'IDLE'}</span>
        </div>
        <div className="model-provenance" aria-label="Model attribution chain">
          <div><span>APPLICATION</span><strong>{model?.application || application?.application || '—'}</strong></div><em>→</em>
          <div><span>RUNTIME</span><strong>{model?.runtime || application?.runtime || '—'}</strong></div><em>→</em>
          <div><span>MODEL</span><strong>{model?.model || 'Not reported'}</strong></div>
        </div>
        {compatibility && <div className={`compatibility-banner fit-${compatibility.state}`}><div><span>HARDWARE FIT</span><strong>{compatibility.label}</strong><small>{compatibility.requiredMemoryGb.toFixed(1)} GB estimated · {compatibility.mode.toUpperCase()}</small></div><div><span>{compatibility.workload === 'embedding' ? 'EMBEDDING WORKLOAD' : 'ESTIMATED GENERATION'}</span><strong>{tokenSpeed(compatibility.estimatedTpsMin, compatibility.estimatedTpsMax, compatibility.workload)}</strong><small>{compatibility.workload === 'embedding' ? 'generation tok/s does not apply' : `${compatibility.confidence} confidence · not a benchmark`}</small></div></div>}
        <div className="ai-primary-stats">
          {model?.status === 'detected' ? <>
            <div><span>Installed model size</span><strong>{selectedModelSize?.value || '—'}</strong><small>{selectedModelSize?.unit === 'size unavailable' ? selectedModelSize.unit : `${selectedModelSize?.unit} on disk`}</small></div>
            <div><span>Estimated memory need</span><strong>{compatibility?.requiredMemoryGb.toFixed(1) || '—'}</strong><small>GB including runtime overhead</small></div>
            <div><span>{compatibility?.workload === 'embedding' ? 'Workload type' : 'Estimated generation'}</span><strong>{compatibility?.workload === 'embedding' ? 'EMBED' : compatibility?.estimatedTpsMax ?? '—'}</strong><small>{compatibility?.workload === 'embedding' ? 'generation speed is not applicable' : 'tok/s upper range · not loaded'}</small></div>
          </> : <>
            <div><span>Model VRAM allocation</span><strong>{modelVramGb.toFixed(2)}</strong><small>GB reported by {model?.application || 'process counters'}</small></div>
            <div><span>Model RAM allocation</span><strong>{allocatedRamGb.toFixed(2)}</strong><small>GB outside VRAM</small></div>
            <div><span>Application power now</span><strong>{appEnergy.toFixed(1)}</strong><small>watts · modeled attribution</small></div>
          </>}
        </div>
        <div className="model-spec-grid">
          <div><span>Family</span><strong>{model?.family || 'Not reported'}</strong></div>
          <div><span>Parameters</span><strong>{model?.parameters || 'Not reported'}</strong></div>
          <div><span>Quantization</span><strong>{model?.quantization || 'Not reported'}</strong></div>
          <div><span>Context capacity</span><strong>{model?.contextLength ? `${model.contextLength.toLocaleString()} tokens` : 'Not reported'}</strong></div>
        </div>
        <div className="inference-chart-head"><div><strong>System GPU activity</strong><span>Live context while this runtime is present — not claimed as token throughput.</span></div><span className="source-chip">{model?.source || 'Operating system process counters'}</span></div>
        <div className="inference-chart"><LineChart values={metrics.history.gpu} tone="violet" height={180} /></div>
      </section>
      <aside className="ai-side-column">
        <article className="panel allocation-card"><p className="eyebrow">APPLICATION FOOTPRINT</p><h3>{application?.application || 'No active application'}</h3><ResourceBar label="GPU" value={application?.gpu || 0} detail={`${(application?.gpu || 0).toFixed(1)}%`} tone="violet" /><ResourceBar label="VRAM" value={totalVramGb ? modelVramGb / totalVramGb * 100 : 0} detail={`${modelVramGb.toFixed(2)} / ${totalVramGb.toFixed(1)} GB`} tone="amber" /><ResourceBar label="RAM" value={metrics.memory?.totalBytes ? (application?.memoryGb || 0) / bytesToGb(metrics.memory.totalBytes) * 100 : 0} detail={`${(application?.memoryGb || 0).toFixed(2)} GB`} tone="mint" /><ResourceBar label="CPU" value={application?.cpu || 0} detail={`${(application?.cpu || 0).toFixed(1)}%`} tone="cyan" /></article>
        <article className="panel runtime-card"><p className="eyebrow">RUNTIME DETAILS</p><dl><div><dt>Application</dt><dd>{model?.application || application?.application || '—'}</dd></div><div><dt>Runtime</dt><dd>{model?.runtime || application?.runtime || '—'}</dd></div><div><dt>Consumer process</dt><dd>{runtimeProcess?.name || 'Not exposed'}</dd></div><div><dt>PID</dt><dd>{runtimeProcess?.pid || 'Not exposed'}</dd></div><div><dt>API evidence</dt><dd>{model?.source || 'Process tree only'}</dd></div></dl></article>
        <article className="panel runtime-card adapter-card"><p className="eyebrow">LOCAL ADAPTERS</p><div className="adapter-list">{(localAI?.adapters || []).map((adapter) => <div key={adapter.id}><i className={adapter.status === 'online' ? 'adapter-online' : ''} /><span><strong>{adapter.name}</strong><small>{adapter.endpoint}</small></span><em>{adapter.status}</em></div>)}</div></article>
      </aside>
      <section className="panel ai-inventory-card">
        <div className="panel-header"><div><p className="eyebrow">LOCAL MODEL CHECKER</p><h2>What this machine can actually run</h2></div><span>{localAI?.installedModelCount || installedModels.length} models inventoried</span></div>
        <div className="inventory-hardware-strip"><span><Server size={15} /><strong>{hardware?.gpu?.model || 'GPU unavailable'}</strong><small>{bytesToGb(hardware?.gpu?.vramBytes).toFixed(1)} GB VRAM</small></span><span><Cpu size={15} /><strong>{hardware?.cpu.brand || 'CPU unavailable'}</strong><small>{hardware?.cpu.physicalCores || hardware?.cpu.cores || 0} physical cores</small></span><span><MemoryStick size={15} /><strong>{bytesToGb(hardware?.memory.totalBytes).toFixed(1)} GB RAM</strong><small>OS reserve included in every verdict</small></span></div>
        <div className="model-inventory-list">
          {installedModels.length === 0 && <div className="ai-empty-state"><Bot size={20} /><div><strong>No local model found yet</strong><span>VISOR checked nine loopback adapters, Ollama manifests, common model libraries and known weight formats.</span></div></div>}
          {installedModels.map((item) => {
            const fit = analyzeLocalModel(item, hardware);
            return <button key={item.id} className={item.id === model?.id ? 'selected' : ''} onClick={() => setSelectedModelId(item.id)}><div className="model-inventory-name"><strong>{item.model}</strong><span>{item.application} · {item.quantization || item.format?.toUpperCase() || 'metadata partial'}</span></div><span className="model-size-cell">{modelSize(item)}<small>{item.status}</small></span><span className={`fit-badge fit-${fit.state}`}>{fit.label}<small>{fit.mode}</small></span><span className="speed-cell">{tokenSpeed(fit.estimatedTpsMin, fit.estimatedTpsMax, fit.workload)}<small>{fit.workload === 'embedding' ? 'no generation tok/s' : `${fit.confidence} confidence`}</small></span></button>;
          })}
        </div>
        <p className="advisor-disclaimer">Predictions are conservative engineering ranges based on model weight, quantization, usable VRAM/RAM, CPU class and execution mode. Run a benchmark to replace the estimate with measured throughput.</p>
      </section>
      <section className="panel ai-services-card">
        <div className="panel-header"><div><p className="eyebrow">AI SERVICE ATTRIBUTION</p><h2>Detected local runtimes and cloud clients</h2></div><span>{services.length} services · live process totals</span></div>
        <div className="cloud-visibility-note"><Cloud size={18} /><div><strong>Cloud billing is not connected</strong><span>For paid APIs and subscriptions, VISOR reports only this device’s client process footprint. It never reads API keys, prompts or encrypted traffic; token counts and provider charges require explicit read-only connectors.</span></div><em>LOCAL FOOTPRINT ONLY</em></div>
        <div className="ai-service-table">
          <div className="ai-service-head"><span>Service</span><span>CPU</span><span>GPU</span><span>RAM</span><span>VRAM</span><span>Power</span></div>
          {services.length === 0 && <div className="ai-empty-state"><Cloud size={20} /><div><strong>No AI service process detected</strong><span>VISOR watches local runtimes, desktop clients and coding agents without reading prompts.</span></div></div>}
          {services.map((service) => <div className="ai-service-row" key={service.id}><div><span className={`service-mode service-${service.execution || 'unknown'}`}>{service.execution === 'local' ? <Server size={13} /> : <Cloud size={13} />}</span><span><strong>{service.application}</strong><small>{service.provider || service.runtime} · {service.processes.length} process{service.processes.length === 1 ? '' : 'es'}</small></span><em>{service.execution || 'unknown'}</em></div><strong>{service.cpu.toFixed(1)}%</strong><strong>{service.gpu.toFixed(1)}%</strong><strong>{service.memoryGb.toFixed(2)} GB</strong><strong>{service.vramGb.toFixed(2)} GB</strong><strong className="service-power">{service.energyWatts.toFixed(1)} W</strong></div>)}
        </div>
      </section>
    </div>
  );
}

function ResourceBar({ label, value, detail, tone }: { label: string; value: number; detail: string; tone: MetricTone }) {
  return <div className={`resource-bar tone-${tone}`}><div><span>{label}</span><strong>{detail}</strong></div><i><b style={{ width: `${value}%` }} /></i></div>;
}

function EnergyView({ energy, processes, agent }: { energy?: EnergyEstimate; processes: ProcessInfo[]; agent?: AgentInfo }) {
  if (!energy) {
    return <section className="panel energy-offline"><div className="insight-icon amber-bg"><Zap size={21} /></div><div><p className="eyebrow">LOCAL COLLECTOR REQUIRED</p><h2>Energy Lens is waiting for live telemetry.</h2><span>Run the VISOR desktop application, or the optional development agent on Windows, to enable energy modeling and process attribution.</span></div></section>;
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

function HistoryView({ metrics, energy, agent }: MetricsProps & { energy?: EnergyEstimate; agent?: AgentInfo }) {
  const [range, setRange] = useState('1 hour');
  const history = metrics.history.cpu;
  const average = (values: number[]) => values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : 0;
  const peak = (values: number[]) => values.length ? Math.max(...values) : 0;
  const uptimeMinutes = Math.floor((agent?.uptimeSeconds || 0) / 60);
  return (
    <div className="history-grid">
      <section className="panel history-chart-card">
        <div className="panel-header"><div><p className="eyebrow">SYSTEM LOAD</p><h2>Performance timeline</h2></div><div className="segmented-control">{['1 hour', '24 hours', '7 days'].map((item) => <button key={item} className={range === item ? 'active' : ''} onClick={() => setRange(item)}>{item}</button>)}</div></div>
        <div className="history-legend"><span><i className="cyan-legend" />CPU average {average(metrics.history.cpu).toFixed(1)}%</span><span><i className="violet-legend" />GPU average {average(metrics.history.gpu).toFixed(1)}%</span></div>
        <div className="history-main-chart"><LineChart values={history} tone="cyan" height={250} /></div>
        <div className="history-axis"><span>{history.length} samples ago</span><span>Rolling live buffer</span><span>Now</span></div>
      </section>
      <section className="peak-grid">
        <PeakCard label="Peak CPU" value={`${peak(metrics.history.cpu).toFixed(1)}%`} time="Rolling buffer" icon={<Cpu />} />
        <PeakCard label="Peak GPU" value={`${peak(metrics.history.gpu).toFixed(1)}%`} time="Rolling buffer" icon={<Gauge />} />
        <PeakCard label="Peak memory" value={`${peak(metrics.history.ram).toFixed(1)}%`} time="Rolling buffer" icon={<MemoryStick />} />
        <PeakCard label="Energy used" value={energy ? `${energy.sessionWh.toFixed(2)} Wh` : '—'} time="Agent session" icon={<Zap />} />
      </section>
      <section className="panel session-card"><div><div className="session-icon"><Clock3 /></div><div><p className="eyebrow">CURRENT COLLECTOR SESSION</p><h3>{agent ? `${Math.floor(uptimeMinutes / 60)}h ${uptimeMinutes % 60}m monitored` : 'Local collector unavailable'}</h3><span>{history.length} live samples in the interface buffer · no fabricated history</span></div></div><span className="source-chip">LOCAL ONLY</span></section>
    </div>
  );
}

function PeakCard({ label, value, time, icon }: { label: string; value: string; time: string; icon: React.ReactNode }) {
  return <article className="panel peak-card"><span>{icon}</span><div><p>{label}</p><strong>{value}</strong><small>{time}</small></div></article>;
}

function AlertsView({ alerts, live, onToggle }: { alerts?: AlertsSnapshot; live: boolean; onToggle: (id: string, enabled: boolean) => Promise<unknown> }) {
  const active = alerts?.active || [];
  const watch = alerts?.watch || [];
  const gaming = alerts?.gaming;
  const formatDuration = (durationMs: number) => {
    const seconds = Math.round(durationMs / 1000);
    if (seconds < 60) return `${seconds} sec`;
    const minutes = Math.floor(seconds / 60);
    const remainder = seconds % 60;
    return remainder ? `${minutes} min ${remainder} sec` : `${minutes} min`;
  };
  const rulePresentation = (metric: string) => metric.includes('Temp')
    ? { icon: <Thermometer />, tone: 'amber' }
    : metric === 'vram'
      ? { icon: <Gauge />, tone: 'violet' }
      : metric === 'cpu'
        ? { icon: <Cpu />, tone: 'cyan' }
        : { icon: <Gauge />, tone: 'rose' };
  return (
    <div className="alerts-grid">
      <section className={`panel alert-summary ${active.length ? 'alert-summary-active' : ''}`}><div className="alert-summary-icon">{active.length ? <ShieldAlert /> : <Check />}</div><div><p className="eyebrow">{active.length ? 'ACTION REQUIRED' : 'LIVE GUARD'}</p><h2>{active.length ? `${active.length} sustained alert${active.length > 1 ? 's' : ''}` : 'No sustained overload detected'}</h2><span>{active.length ? active.map((item) => `${item.title}: ${item.value}${item.unit}`).join(' · ') : 'VISOR waits for sustained conditions before interrupting you.'}</span></div><span className="policy-live"><i /> {live ? 'Agent policy live' : 'Agent offline'}</span></section>

      <section className={`panel gaming-guard ${gaming?.active ? 'gaming-guard-active' : ''}`}><div className="gaming-guard-icon"><Gamepad2 /></div><div><p className="eyebrow">GAME-AWARE SUPPRESSION</p><h3>{gaming?.active ? `${gaming.processName} detected` : 'Gaming mode standing by'}</h3><span>{gaming?.active ? 'Sustained CPU/GPU load alerts are muted while the game is active. Thermal and VRAM protection remain live.' : 'VISOR only suppresses load alerts when a real game process is actively using CPU or GPU.'}</span></div><strong>{gaming?.active ? 'LOAD ALERTS MUTED' : 'ARMED'}</strong></section>

      {watch.length > 0 && <section className="panel alert-watch-card"><div className="panel-header"><div><p className="eyebrow">CONDITION WATCH</p><h2>Thresholds currently observed</h2></div><span>{watch.length} signal{watch.length > 1 ? 's' : ''}</span></div><div className="alert-watch-list">{watch.map((item) => <div key={item.id}><span className={`watch-dot ${item.suppressed ? 'watch-suppressed' : ''}`} /><div><strong>{item.title}</strong><small>{item.suppressed ? 'Suppressed by active gaming context' : `${item.value}${item.unit} · ${item.progress}% of sustained duration`}</small></div><em>{item.suppressed ? 'MUTED' : `${item.progress}%`}</em></div>)}</div></section>}

      <section className="panel rules-card"><div className="panel-header"><div><p className="eyebrow">SUSTAINED POLICIES</p><h2>Alert rules</h2></div><span>{alerts?.rules.filter((rule) => rule.enabled).length || 0} enabled</span></div><div className="rules-list">{(alerts?.rules || []).map((rule) => {
        const presentation = rulePresentation(rule.metric);
        const state = active.find((item) => item.id === rule.id) || watch.find((item) => item.id === rule.id);
        return <div className="rule-row" key={rule.id}><span className={`rule-icon rule-${presentation.tone}`}>{presentation.icon}</span><div><strong>{rule.title}</strong><span>Above {rule.threshold}{rule.unit} for {formatDuration(rule.durationMs)}{rule.suppressDuringGaming ? ' · ignored during active gaming' : ' · always protected'}</span>{state && !state.suppressed && <i className="rule-progress"><b style={{ width: `${state.progress}%` }} /></i>}</div><button aria-label={`${rule.enabled ? 'Disable' : 'Enable'} ${rule.title}`} disabled={!live} className={`switch ${rule.enabled ? 'switch-on' : ''}`} onClick={() => void onToggle(rule.id, !rule.enabled)}><span /></button></div>;
      })}</div></section>

      <section className="panel recent-alert"><div className="recent-alert-head"><span><Info size={16} /></span><div><strong>{alerts?.recent[0]?.title || 'No resolved alert in this agent session'}</strong><small>{alerts?.recent[0] ? `Peak ${alerts.recent[0].peak}${alerts.recent[0].unit} · ${new Date(alerts.recent[0].resolvedAt).toLocaleTimeString()}` : 'Evidence appears here after a sustained alert resolves.'}</small></div>{alerts?.recent[0] && <em>Resolved</em>}</div><p>{alerts?.recent[0] ? 'The condition returned below its threshold. VISOR retained the peak and timing for diagnosis.' : 'Short spikes are intentionally ignored so notifications stay useful and quiet.'}</p></section>
    </div>
  );
}

function SettingsView({ theme, setTheme, metrics, hardware, agent, source }: { theme: ThemeId; setTheme: (theme: ThemeId) => void; metrics: MetricsProps['metrics']; hardware?: HardwareInfo; agent?: AgentInfo; source?: 'windows-agent' | 'tauri-native' }) {
  const platform = hardware?.os.platform || 'operating system';
  const themes: Array<{ id: ThemeId; name: string; description: string; icon: React.ReactNode }> = [
    { id: 'studio', name: 'Studio', description: 'Graphite glass, restrained color', icon: <Sparkles size={15} /> },
    { id: 'porcelain', name: 'Porcelain', description: 'Bright, soft and editorial', icon: <Sun size={15} /> },
    { id: 'cyber', name: 'Cyberdeck', description: 'Neon telemetry, precision grid', icon: <Activity size={15} /> },
    { id: 'retro', name: 'Retro terminal', description: 'Warm phosphor, modern clarity', icon: <Clock3 size={15} /> },
  ];
  return (
    <div className="settings-grid">
      <section className="panel settings-card appearance-card"><div className="settings-heading"><span><Palette /></span><div><h2>Appearance</h2><p>Four distinct art directions. The information hierarchy and contrast stay consistent.</p></div></div><div className="theme-choices">{themes.map((item) => <button key={item.id} aria-pressed={theme === item.id} className={theme === item.id ? 'active' : ''} onClick={() => setTheme(item.id)}><span className={`theme-preview theme-${item.id}`}><i /><b /><em /></span><strong>{item.icon}{item.name}</strong><small>{item.description}</small></button>)}</div></section>
      <section className="panel settings-card"><div className="settings-heading"><span><Activity /></span><div><h2>Live data sources</h2><p>VISOR exposes coverage honestly. Missing hardware sensors are never replaced with invented values.</p></div></div><SettingStatus label="Collector runtime" detail={`One batched ${platform} snapshot every second`} value={agent ? (source === 'tauri-native' ? 'NATIVE' : 'LOOPBACK') : 'OFFLINE'} tone={agent ? 'live' : 'off'} /><SettingStatus label="Collector overhead" detail={agent ? `${agent.sampleDurationMs} ms last sample · PID ${agent.pid}` : 'Waiting for collector diagnostics'} value={agent ? `${agent.cpuPercent.toFixed(1)}% · ${agent.memoryMb.toFixed(0)} MB` : 'UNAVAILABLE'} tone={agent ? 'live' : 'off'} /><SettingStatus label="Per-process GPU attribution" detail={`Vendor and ${platform} GPU counters when available`} value={agent?.gpuAttributionAvailable ? 'AVAILABLE' : 'UNAVAILABLE'} tone={agent?.gpuAttributionAvailable ? 'live' : 'off'} /><SettingStatus label="CPU temperature" detail={metrics.sensorSources?.cpu || 'No compatible native hardware sensor is exposed on this system'} value={metrics.cpuTemp > 0 ? `${Math.round(metrics.cpuTemp)}°C` : 'UNAVAILABLE'} tone={metrics.cpuTemp > 0 ? 'live' : 'off'} /><SettingStatus label="SSD temperature" detail={metrics.storageTemperatures?.[0]?.source || 'No compatible storage temperature sensor is exposed on this system'} value={(metrics.ssdTemp || 0) > 0 ? `${Math.round(metrics.ssdTemp || 0)}°C` : 'UNAVAILABLE'} tone={(metrics.ssdTemp || 0) > 0 ? 'live' : 'off'} /></section>
      <section className="panel settings-card"><div className="settings-heading"><span><ShieldCheck /></span><div><h2>Privacy and trust</h2><p>Telemetry and runtime probes stay on this device.</p></div></div><SettingStatus label="Network boundary" detail={source === 'tauri-native' ? 'Native IPC; no telemetry HTTP server is started' : 'Browser agent bound to IPv4 loopback only'} value={source === 'tauri-native' ? 'IPC ONLY' : '127.0.0.1'} tone="live" /><SettingStatus label="External telemetry" detail="VISOR sends no hardware or model data to a remote service" value="OFF" tone="live" /></section>
    </div>
  );
}

function SettingStatus({ label, detail, value, tone }: { label: string; detail: string; value: string; tone: 'live' | 'off' }) {
  return <div className="setting-row"><div><strong>{label}</strong><span>{detail}</span></div><em className={`setting-status setting-status-${tone}`}>{value}</em></div>;
}

function CommandPalette({ onClose, onNavigate }: { onClose: () => void; onNavigate: (view: ViewId) => void }) {
  return (
    <div className="command-backdrop" onMouseDown={onClose}>
      <div className="command-palette" onMouseDown={(event) => event.stopPropagation()}>
        <div className="command-search"><Search size={18} /><input autoFocus placeholder="Search VISOR or type a command…" /><button onClick={onClose}><X size={16} /></button></div>
        <p>QUICK NAVIGATION</p>
        <button onClick={() => onNavigate('processes')}><span><Cpu size={18} /> View all processes</span><Command size={14} /></button>
        <button onClick={() => onNavigate('ai')}><span><Bot size={18} /> Open AI workloads</span><em>Local adapters</em></button>
        <button onClick={() => onNavigate('performance')}><span><Gauge size={18} /> Inspect GPU performance</span><em>Live telemetry</em></button>
        <button onClick={() => onNavigate('alerts')}><span><Bell size={18} /> Configure smart alerts</span></button>
        <div className="command-footer"><span><kbd>↑↓</kbd> Navigate</span><span><kbd>Enter</kbd> Select</span><span><kbd>Esc</kbd> Close</span></div>
      </div>
    </div>
  );
}

export default App;
