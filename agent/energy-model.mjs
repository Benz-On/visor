const clamp = (value, min, max) => Math.min(max, Math.max(min, value));
const finite = (value, fallback = 0) => Number.isFinite(Number(value)) ? Number(value) : fallback;

// Keep in sync with `infer_cpu_tdp` in src-tauri/src/energy.rs. The suffix of
// the model number sets the power class for Intel and AMD mobile parts.
function modelSuffix(brand) {
  const matches = [...String(brand || '').toLowerCase().matchAll(/(\d{3,})([a-z]{1,2})/g)];
  return matches.length ? matches[matches.length - 1][2] : null;
}

export function inferCpuTdp(cpu = {}) {
  const brand = String(cpu.brand || cpu.model || '').toLowerCase();
  const cores = finite(cpu.physicalCores || cpu.cores, 8);

  switch (modelSuffix(brand)) {
    case 'u':
      return 28;
    case 'h':
    case 'hs':
    case 'hx':
      return 55;
    default:
      if (cores <= 4) return 65;
      if (cores <= 8) return 105;
      if (cores <= 16) return 170;
      return 230;
  }
}

export function estimateEnergy(input) {
  const cpuLoad = clamp(finite(input.cpuLoad), 0, 100);
  const gpuLoad = clamp(finite(input.gpuLoad), 0, 100);
  const memoryTotalGb = Math.max(1, finite(input.memoryTotalGb, 16));
  const diskActivity = clamp(finite(input.diskActivity), 0, 100);
  const cpuTdp = Math.max(15, finite(input.cpuTdp, 105));
  const speedRatio = clamp(finite(input.cpuSpeedRatio, 1), 0.45, 1.35);
  const measuredCpuPower = finite(input.measuredCpuPower, 0);
  const measuredGpuPower = finite(input.measuredGpuPower, 0);
  const gpuPowerLimit = Math.max(30, finite(input.gpuPowerLimit, 220));

  const cpuIdle = Math.max(5, cpuTdp * 0.075);
  // Keep in sync with `estimate` in src-tauri/src/energy.rs: an on-die package
  // reading replaces the TDP curve rather than blending into it.
  const cpu = measuredCpuPower > 0
    ? measuredCpuPower
    : cpuIdle + (cpuTdp - cpuIdle) * Math.pow(cpuLoad / 100, 0.78) * speedRatio;
  const gpu = measuredGpuPower > 0
    ? measuredGpuPower
    : gpuPowerLimit * (0.1 + 0.9 * Math.pow(gpuLoad / 100, 0.92));
  const memory = 2 + memoryTotalGb * 0.12;
  const storage = 2.5 + diskActivity * 0.055;
  const platform = 30;
  const componentTotal = cpu + gpu + memory + storage + platform;
  const psuEfficiency = componentTotal > 350 ? 0.91 : componentTotal > 150 ? 0.89 : 0.86;
  const total = componentTotal / psuEfficiency;
  const cpuMeasured = measuredCpuPower > 0;
  const gpuMeasured = measuredGpuPower > 0;
  const bothMeasured = cpuMeasured && gpuMeasured;

  return {
    watts: Math.round(total * 10) / 10,
    cpuMeasured,
    gpuMeasured,
    confidence: bothMeasured ? 'measured' : cpuMeasured || gpuMeasured ? 'hybrid' : 'estimated',
    confidenceScore: bothMeasured ? 88 : cpuMeasured ? 78 : gpuMeasured ? 76 : 54,
    methodology: bothMeasured
      ? 'CPU package and GPU measured by on-die telemetry; memory, storage and platform modeled from live load.'
      : cpuMeasured
        ? 'CPU package measured by on-die telemetry; GPU and platform modeled from live load.'
        : gpuMeasured
          ? 'GPU measured by vendor telemetry; CPU and platform modeled from live load.'
          : 'CPU, GPU and platform modeled from live utilization and hardware limits.',
    breakdown: {
      cpu: Math.round(cpu * 10) / 10,
      gpu: Math.round(gpu * 10) / 10,
      memory: Math.round(memory * 10) / 10,
      storage: Math.round(storage * 10) / 10,
      platform: Math.round(platform * 10) / 10,
      conversionLoss: Math.round((total - componentTotal) * 10) / 10,
    },
  };
}

export function attributeProcessEnergy(processes, energy, usedMemoryGb, activity = {}) {
  const cpuPool = Math.max(0, finite(energy.breakdown?.cpu));
  const gpuPool = Math.max(0, finite(energy.breakdown?.gpu));
  const memoryPool = Math.max(0, finite(energy.breakdown?.memory));
  const observedCpu = processes.reduce((sum, process) => sum + Math.max(0, finite(process.cpu)), 0);
  const observedGpu = processes.reduce((sum, process) => sum + Math.max(0, finite(process.gpu)), 0);
  const totalCpu = Math.max(observedCpu, finite(activity.systemCpuLoad));
  const totalGpu = Math.max(observedGpu, finite(activity.systemGpuLoad));
  const memoryBase = Math.max(0.25, finite(usedMemoryGb, 1));

  return processes.map((process) => {
    const cpuShare = totalCpu > 0 ? Math.max(0, finite(process.cpu)) / totalCpu : 0;
    const gpuShare = totalGpu > 0 ? Math.max(0, finite(process.gpu)) / totalGpu : 0;
    const memoryShare = clamp(Math.max(0, finite(process.memory)) / memoryBase, 0, 1);
    const watts = cpuPool * cpuShare + gpuPool * gpuShare + memoryPool * memoryShare;
    return { ...process, energyWatts: Math.round(watts * 10) / 10 };
  });
}

export function projectEnergy(watts, sessionWh, tariffPerKwh = 0.25, carbonGramsPerKwh = 56) {
  const safeWatts = Math.max(0, finite(watts));
  const dailyKwh = safeWatts * 24 / 1000;
  return {
    sessionWh: Math.round(Math.max(0, finite(sessionWh)) * 100) / 100,
    hourlyKwh: Math.round(safeWatts / 10) / 100,
    dailyKwh: Math.round(dailyKwh * 1000) / 1000,
    dailyCost: Math.round(dailyKwh * tariffPerKwh * 100) / 100,
    monthlyCost: Math.round(dailyKwh * tariffPerKwh * 30 * 100) / 100,
    dailyCarbonGrams: Math.round(dailyKwh * carbonGramsPerKwh),
    tariffPerKwh,
    carbonGramsPerKwh,
  };
}
