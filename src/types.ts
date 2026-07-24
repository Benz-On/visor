export type MetricTone = 'cyan' | 'violet' | 'mint' | 'amber' | 'rose';

export type ThemeId = 'studio' | 'porcelain' | 'cyber' | 'retro';

export type MetricKey = 'cpu' | 'gpu' | 'ram' | 'vram';

export interface LiveMetrics {
  cpu: number;
  gpu: number;
  ram: number;
  vram: number;
  cpuTemp: number;
  gpuTemp: number;
  ssdTemp?: number;
  storageTemperatures?: StorageTemperature[];
  sensorSources?: { cpu: string | null; gpu: string | null; storage: string[] };
  cpuPower: number;
  gpuPower: number;
  download: number;
  upload: number;
  diskRead: number;
  diskWrite: number;
  cpuSpeedGhz?: number;
  cpuCoreLoads?: number[];
  diskActivity?: number;
  memory?: {
    totalBytes: number;
    usedBytes: number;
    availableBytes: number;
    cachedBytes: number;
    swapTotalBytes: number;
    swapUsedBytes: number;
  };
  gpuMemory?: { totalBytes: number; usedBytes: number };
  history: Record<MetricKey | 'network' | 'disk', number[]>;
}

export interface ProcessInfo {
  id: number;
  name: string;
  subtitle: string;
  icon: string;
  color: string;
  cpu: number;
  gpu: number;
  memory: number;
  vram: number;
  disk: number;
  network: number;
  power: 'Very low' | 'Low' | 'Moderate' | 'High';
  kind?: 'AI' | 'Game' | 'Creative';
  energyWatts?: number;
  protected?: boolean;
  priority?: number;
  state?: string;
  path?: string;
  handles?: number;
  threads?: number;
  gpuEngines?: Record<string, number>;
  aiApplication?: string;
  aiRuntime?: string;
  aiRole?: string;
}

export interface StorageTemperature {
  name: string;
  deviceId: string | null;
  temperature: number;
  temperatureMax: number;
  wear: number | null;
  source: string;
}

export interface LocalAiProcess {
  pid: number;
  name: string;
  role: string;
  cpu: number;
  gpu: number;
  memoryGb?: number;
  vramGb: number;
  energyWatts: number;
}

export interface LocalModelInfo {
  id: string;
  application: string;
  runtime: string;
  model: string;
  status: 'active' | 'loaded' | 'detected';
  source: string;
  confidence: number;
  family: string;
  parameters: string;
  quantization: string;
  format: string;
  contextLength: number;
  sizeBytes?: number;
  allocatedBytes: number;
  allocatedVramBytes: number;
  allocatedRamBytes: number;
  allocatedVramGb: number;
  expiresAt: string | null;
  installed?: boolean;
  location?: string;
  process: LocalAiProcess | null;
  applicationEnergyWatts: number | null;
  applicationCpu: number;
  applicationGpu: number;
}

export interface LocalAiApplication {
  id: string;
  application: string;
  runtime: string;
  provider?: string;
  category?: 'local-runtime' | 'cloud-client' | 'coding-agent' | 'creative-ai';
  execution?: 'local' | 'cloud' | 'hybrid' | 'unknown';
  processes: LocalAiProcess[];
  cpu: number;
  gpu: number;
  memoryGb: number;
  vramGb: number;
  energyWatts: number;
  active: boolean;
  running?: boolean;
}

export interface LocalAiSnapshot {
  scannedAt: string | null;
  adapters: Array<{ id: string; name: string; status: 'online' | 'offline'; endpoint: string }>;
  models: LocalModelInfo[];
  applications: LocalAiApplication[];
  modelRoots?: Array<{ label: string; modelCount: number }>;
  activeModelCount: number;
  loadedModelCount: number;
  installedModelCount?: number;
  serviceCount?: number;
}

export interface ModelCompatibility {
  workload: 'generation' | 'embedding';
  state: 'excellent' | 'good' | 'limited' | 'too-large' | 'unknown';
  label: string;
  mode: 'gpu' | 'hybrid' | 'cpu' | 'unavailable';
  requiredMemoryGb: number;
  availableVramGb: number;
  availableRamGb: number;
  estimatedTpsMin: number | null;
  estimatedTpsMax: number | null;
  confidence: 'medium' | 'low';
  reason: string;
}

export interface AlertRule {
  id: string;
  title: string;
  metric: string;
  threshold: number;
  durationMs: number;
  unit: string;
  suppressDuringGaming: boolean;
  enabled: boolean;
  severity: 'warning' | 'critical';
}

export interface AlertState extends AlertRule {
  value: number;
  sensorAvailable: boolean;
  suppressed: boolean;
  elapsedMs: number;
  progress: number;
  active: boolean;
  since: string | null;
}

export interface AlertsSnapshot {
  active: AlertState[];
  watch: AlertState[];
  recent: Array<{ id: string; ruleId: string; title: string; peak: number; unit: string; startedAt: string; resolvedAt: string; gamingSuppressed: boolean }>;
  gaming: { active: boolean; processName: string | null; pid: number | null; suppressedRuleIds: string[] };
  rules: AlertRule[];
}

export interface EnergyEstimate {
  watts: number;
  confidence: 'measured' | 'hybrid' | 'estimated';
  confidenceScore: number;
  methodology: string;
  sessionWh: number;
  hourlyKwh: number;
  dailyKwh: number;
  dailyCost: number;
  monthlyCost: number;
  dailyCarbonGrams: number;
  tariffPerKwh: number;
  carbonGramsPerKwh: number;
  breakdown: {
    cpu: number;
    gpu: number;
    memory: number;
    storage: number;
    platform: number;
    conversionLoss: number;
  };
}

export interface HardwareInfo {
  system: { manufacturer: string; model: string; version: string };
  os: { platform: string; distro: string; release: string; build: string; arch: string; hostname: string };
  cpu: {
    manufacturer: string;
    brand: string;
    cores: number;
    physicalCores: number;
    speed: number;
    speedMax: number;
    estimatedTdp: number;
  };
  gpu: null | {
    vendor: string;
    model: string;
    vramBytes: number;
    driverVersion: string;
    powerLimit: number;
  };
  memory: { totalBytes: number; modules: Array<{ sizeBytes: number; type: string; clockMhz: number }> };
  storage: Array<{ name: string; type: string; sizeBytes: number; smartStatus: string; temperature?: number; temperatureSource?: string | null }>;
  displays: Array<{ model: string; main: boolean; resolution: string; refreshRate: number }>;
}

export interface AgentInfo {
  cpuPercent: number;
  memoryMb: number;
  pid: number;
  uptimeSeconds: number;
  sampleDurationMs: number;
  gpuAttributionAvailable: boolean;
  sensorAttributionAvailable: boolean;
  lastError: string | null;
}

export interface SystemSnapshot {
  timestamp: string;
  source: 'windows-agent' | 'tauri-native';
  pollMs: number;
  hardware: HardwareInfo;
  metrics: Omit<LiveMetrics, 'history'>;
  energy: EnergyEstimate;
  processes: ProcessInfo[];
  processCounts: { all: number; running: number; blocked: number; sleeping: number };
  localAI: LocalAiSnapshot;
  alerts: AlertsSnapshot;
  agent: AgentInfo;
}
