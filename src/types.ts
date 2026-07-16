export type MetricTone = 'cyan' | 'violet' | 'mint' | 'amber' | 'rose';

export type MetricKey = 'cpu' | 'gpu' | 'ram' | 'vram';

export interface LiveMetrics {
  cpu: number;
  gpu: number;
  ram: number;
  vram: number;
  cpuTemp: number;
  gpuTemp: number;
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
  storage: Array<{ name: string; type: string; sizeBytes: number; smartStatus: string }>;
  displays: Array<{ model: string; main: boolean; resolution: string; refreshRate: number }>;
}

export interface AgentInfo {
  cpuPercent: number;
  memoryMb: number;
  pid: number;
  uptimeSeconds: number;
  sampleDurationMs: number;
  gpuAttributionAvailable: boolean;
  lastError: string | null;
}

export interface SystemSnapshot {
  timestamp: string;
  source: 'windows-agent';
  pollMs: number;
  hardware: HardwareInfo;
  metrics: Omit<LiveMetrics, 'history'>;
  energy: EnergyEstimate;
  processes: ProcessInfo[];
  processCounts: { all: number; running: number; blocked: number; sleeping: number };
  agent: AgentInfo;
}
