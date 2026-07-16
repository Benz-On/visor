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
}
