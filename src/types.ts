export type MetricTone = 'cyan' | 'violet' | 'mint' | 'amber' | 'rose';

export type ThemeId = 'studio' | 'porcelain' | 'cyber' | 'retro';

export type MetricKey = 'cpu' | 'gpu' | 'ram' | 'vram';

export interface InferenceThroughputInfo {
  decodeTokensPerSecond: number | null;
  prefillTokensPerSecond: number | null;
  lastTokens: number | null;
  evidence: 'metrics-endpoint' | 'runtime-log' | 'unavailable' | string;
  observedAt: string | null;
}

export interface LiveMetrics {
  cpu: number;
  gpu: number;
  ram: number;
  vram: number;
  cpuTemp: number;
  gpuTemp: number;
  ssdTemp?: number;
  storageTemperatures?: StorageTemperature[];
  temperatureReadings?: TemperatureReading[];
  hardwareSensors?: HardwareSensorReading[];
  sensorGuidance?: string | null;
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
    committedBytes?: number;
    commitLimitBytes?: number;
    pagedPoolBytes?: number;
    nonPagedPoolBytes?: number;
    pagesPerSecond?: number;
    source?: string;
    swapTotalBytes: number;
    swapUsedBytes: number;
  };
  gpuMemory?: { totalBytes: number; usedBytes: number };
  /** Measured inference throughput; null rates mean no generation observed. */
  throughput?: InferenceThroughputInfo;
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
  memoryPercent?: number;
  workingSetBytes?: number;
  privateBytes?: number;
  virtualBytes?: number;
  vram: number;
  disk: number;
  diskRead?: number;
  diskWrite?: number;
  diskReadTotalBytes?: number;
  diskWriteTotalBytes?: number;
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
  parentId?: number;
  command?: string;
  startedAt?: string | null;
  uptimeSeconds?: number;
  responding?: boolean | null;
  gpuEngines?: Record<string, number>;
  aiApplication?: string;
  aiRuntime?: string;
  aiRole?: string;
}

export interface TemperatureReading {
  component: 'CPU' | 'GPU' | 'Storage' | 'System' | 'Other';
  name: string;
  value: number;
  min: number | null;
  max: number | null;
  source: string;
  accuracy: 'hardware-monitor' | 'graphics-driver' | 'firmware-zone' | string;
}

export interface HardwareSensorReading {
  component: 'CPU' | 'GPU' | 'Memory' | 'Storage' | 'Network' | 'System' | 'Other' | string;
  name: string;
  sensorType: 'Temperature' | 'Load' | 'Clock' | 'Fan' | 'Voltage' | 'Current' | 'Power' | 'Energy' | 'Throughput' | 'Data' | string;
  value: number;
  min: number | null;
  max: number | null;
  unit: string;
  source: string;
  accuracy: 'hardware-monitor' | 'graphics-driver' | 'os-counter' | 'modeled-estimate' | string;
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
  /** Exact GGUF header metadata — present only when the blob was readable. */
  parameterCountExact?: number;
  weightBytesExact?: number;
  kvHeads?: number | null;
  kvHeadCountExact?: boolean;
  layers?: number | null;
  experts?: number | null;
  activeExperts?: number | null;
  moe?: boolean;
  activeParameterRatio?: number;
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
  cloudProviders?: CloudProviderStatus[];
}

export interface CloudProviderStatus {
  id: string;
  name: string;
  provider: string;
  detected: boolean;
  credentialConfigured: boolean;
  credentialSignals: string[];
  localProcessCount: number;
  cpu: number;
  gpu: number;
  memoryGb: number;
  energyWatts: number;
  billingVisible: false;
}

export interface ModelCompatibility {
  workload: 'generation' | 'embedding';
  state: 'excellent' | 'good' | 'limited' | 'too-large' | 'unknown';
  label: string;
  mode: 'gpu' | 'hybrid' | 'cpu' | 'paging' | 'unavailable';
  requiredMemoryGb: number;
  modelWeightGb: number;
  runtimeOverheadGb: number;
  kvCacheGb: number;
  totalRamGb: number;
  totalVramGb: number;
  totalCombinedMemoryGb: number;
  availableVramGb: number;
  availableRamGb: number;
  usableCombinedMemoryGb: number;
  ramReserveGb: number;
  vramReserveGb: number;
  isUnifiedMemory: boolean;
  assumedContextTokens: number;
  gpuOffloadPercent: number;
  memoryDeficitGb: number;
  estimatedTpsMin: number | null;
  estimatedTpsMax: number | null;
  estimatedTpsCenter: number | null;
  effectiveBandwidthGbps: number | null;
  confidence: 'medium' | 'low';
  confidenceScore: number;
  bottleneck: 'GPU memory bandwidth' | 'system RAM / split offload' | 'system memory bandwidth' | 'storage paging' | 'metadata';
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
  collectedAt?: string | null;
  system: { manufacturer: string; model: string; version: string };
  motherboard?: { manufacturer: string; model: string; version: string };
  bios?: { vendor: string; version: string; date: string; smbiosVersion: string };
  os: { platform: string; distro: string; release: string; build: string; arch: string; hostname: string };
  cpu: {
    manufacturer: string;
    brand: string;
    cores: number;
    physicalCores: number;
    speed: number;
    speedMax: number;
    socket?: string;
    l2CacheBytes?: number;
    l3CacheBytes?: number;
    virtualization?: boolean;
    estimatedTdp: number;
  };
  gpu: null | {
    vendor: string;
    model: string;
    vramBytes: number;
    driverVersion: string;
    powerLimit: number;
    driverDate?: string;
    videoMode?: string;
    resolution?: string;
    refreshRate?: number;
    status?: string;
  };
  gpus?: Array<NonNullable<HardwareInfo['gpu']>>;
  memory: { totalBytes: number; modules: Array<{ sizeBytes: number; type: string; clockMhz: number; ratedClockMhz?: number; manufacturer?: string; partNumber?: string; bank?: string; slot?: string; formFactor?: number }> };
  storage: Array<{ name: string; type: string; sizeBytes: number; smartStatus: string; busType?: string; firmware?: string; partitions?: number; temperature?: number; temperatureSource?: string | null }>;
  networks?: Array<{ name: string; manufacturer: string; type: string; speedBits: number; connection: string; status: string }>;
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
