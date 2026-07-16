import { useEffect, useState } from 'react';
import type { LiveMetrics, MetricKey } from '../types';

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
const jitter = (value: number, amount: number, min: number, max: number) =>
  clamp(value + (Math.random() - 0.5) * amount, min, max);

const seed = (length: number, base: number, wave: number) =>
  Array.from({ length }, (_, index) =>
    clamp(base + Math.sin(index / 4.2) * wave + (Math.random() - 0.5) * wave, 2, 98),
  );

const initial: LiveMetrics = {
  cpu: 47,
  gpu: 78,
  ram: 68,
  vram: 76,
  cpuTemp: 64,
  gpuTemp: 72,
  cpuPower: 86,
  gpuPower: 241,
  download: 18.4,
  upload: 2.1,
  diskRead: 124,
  diskWrite: 38,
  history: {
    cpu: seed(34, 46, 13),
    gpu: seed(34, 72, 15),
    ram: seed(34, 67, 3),
    vram: seed(34, 74, 4),
    network: seed(34, 38, 25),
    disk: seed(34, 42, 28),
  },
};

export function useTelemetry(paused: boolean) {
  const [metrics, setMetrics] = useState<LiveMetrics>(initial);

  useEffect(() => {
    if (paused) return;

    const timer = window.setInterval(() => {
      setMetrics((previous) => {
        const nextValues: Record<MetricKey, number> = {
          cpu: jitter(previous.cpu, 13, 12, 92),
          gpu: jitter(previous.gpu, 11, 25, 98),
          ram: jitter(previous.ram, 1.2, 52, 86),
          vram: jitter(previous.vram, 1.4, 58, 94),
        };

        const append = (values: number[], value: number) => [...values.slice(-33), value];

        return {
          ...previous,
          ...nextValues,
          cpuTemp: jitter(previous.cpuTemp, 2.4, 47, 84),
          gpuTemp: jitter(previous.gpuTemp, 1.8, 54, 83),
          cpuPower: jitter(previous.cpuPower, 12, 34, 138),
          gpuPower: jitter(previous.gpuPower, 18, 78, 315),
          download: jitter(previous.download, 8, 0.4, 68),
          upload: jitter(previous.upload, 2, 0.1, 14),
          diskRead: jitter(previous.diskRead, 44, 4, 320),
          diskWrite: jitter(previous.diskWrite, 24, 1, 180),
          history: {
            cpu: append(previous.history.cpu, nextValues.cpu),
            gpu: append(previous.history.gpu, nextValues.gpu),
            ram: append(previous.history.ram, nextValues.ram),
            vram: append(previous.history.vram, nextValues.vram),
            network: append(previous.history.network, previous.download * 1.8),
            disk: append(previous.history.disk, previous.diskRead / 3.2),
          },
        };
      });
    }, 900);

    return () => window.clearInterval(timer);
  }, [paused]);

  return metrics;
}
