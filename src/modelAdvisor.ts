import type { HardwareInfo, LocalModelInfo, ModelCompatibility } from './types';

const GIB = 1024 ** 3;
const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

function parameterBillions(model: LocalModelInfo): number | null {
  const explicit = model.parameters?.match(/(\d+(?:\.\d+)?)\s*[bB]/)?.[1];
  if (explicit) return Number(explicit);
  const fromName = model.model.match(/(?:^|[-_.])(\d+(?:\.\d+)?)\s*[bB](?:[-_.]|$)/)?.[1];
  if (fromName) return Number(fromName);

  const sizeGb = (model.sizeBytes || model.allocatedBytes || 0) / GIB;
  if (!sizeGb) return null;
  const quantization = `${model.quantization} ${model.model}`.toUpperCase();
  const bits = /F32/.test(quantization) ? 32
    : /(?:BF16|F16)/.test(quantization) ? 16
      : /Q8/.test(quantization) ? 8.5
        : /Q6/.test(quantization) ? 6.5
          : /Q5/.test(quantization) ? 5.5
            : /Q4/.test(quantization) ? 4.7
              : /Q3/.test(quantization) ? 3.7
                : /Q2/.test(quantization) ? 2.8
                  : 5;
  return sizeGb * 8 / bits;
}

function gpuFactor(modelName = '', vramGb = 0): number {
  const gpu = modelName.toLowerCase();
  if (/rtx 5090/.test(gpu)) return 2.05;
  if (/rtx 4090/.test(gpu)) return 1.75;
  if (/rtx 5080/.test(gpu)) return 1.55;
  if (/rtx 5070 ti/.test(gpu)) return 1.22;
  if (/rtx 4080/.test(gpu)) return 1.2;
  if (/rtx 5070/.test(gpu)) return 1.02;
  if (/rtx 3090/.test(gpu)) return 1.05;
  if (/rtx 4070 ti/.test(gpu)) return .96;
  if (/rtx 4070/.test(gpu)) return .78;
  if (/rtx 3080/.test(gpu)) return .72;
  if (/rtx 4060/.test(gpu)) return .52;
  if (/radeon.*(?:7900|9070)/.test(gpu)) return .88;
  if (/apple.*(?:m4|max|ultra)/.test(gpu)) return 1.05;
  return clamp(Math.sqrt(Math.max(vramGb, 1) / 12) * .82, .35, 1.2);
}

function quantizationFactor(model: LocalModelInfo): number {
  const quantization = `${model.quantization} ${model.model}`.toUpperCase();
  if (/F32/.test(quantization)) return .38;
  if (/(?:BF16|F16)/.test(quantization)) return .55;
  if (/Q8/.test(quantization)) return .76;
  if (/Q6/.test(quantization)) return .88;
  if (/Q5/.test(quantization)) return .95;
  return 1;
}

function roundTps(value: number) {
  if (value >= 100) return Math.round(value / 5) * 5;
  if (value >= 20) return Math.round(value);
  return Math.round(value * 10) / 10;
}

export function analyzeLocalModel(model: LocalModelInfo, hardware?: HardwareInfo): ModelCompatibility {
  const workload: ModelCompatibility['workload'] = /(?:^|[-_/.])(embed|embedding|rerank|bge|e5|gte)(?:[-_/.]|$)/i.test(model.model)
    ? 'embedding'
    : 'generation';
  const totalRamGb = (hardware?.memory.totalBytes || 0) / GIB;
  const totalVramGb = (hardware?.gpu?.vramBytes || 0) / GIB;
  const availableRamGb = Math.max(0, totalRamGb - Math.max(6, totalRamGb * .2));
  const availableVramGb = Math.max(0, totalVramGb - Math.max(1.5, totalVramGb * .12));
  const parameters = parameterBillions(model);
  const weightGb = (model.sizeBytes || model.allocatedBytes || 0) / GIB;

  if ((!weightGb && !parameters) || !totalRamGb) {
    return {
      workload,
      state: 'unknown', label: 'Needs metadata', mode: 'unavailable', requiredMemoryGb: 0,
      availableVramGb, availableRamGb, estimatedTpsMin: null, estimatedTpsMax: null,
      confidence: 'low', reason: 'The runtime did not expose enough size or parameter metadata for a responsible estimate.',
    };
  }

  const inferredWeightGb = weightGb || Math.max(.2, (parameters || 0) * .62);
  const runtimeOverheadGb = !parameters ? 1.2 : parameters <= 1 ? .4 : parameters <= 8 ? .9 : parameters <= 20 ? 1.4 : parameters <= 40 ? 2 : parameters <= 70 ? 3 : 4.5;
  const requiredMemoryGb = inferredWeightGb * 1.1 + runtimeOverheadGb;
  const combinedAvailableGb = availableVramGb + availableRamGb * .78;

  let state: ModelCompatibility['state'];
  let mode: ModelCompatibility['mode'];
  if (availableVramGb > 0 && requiredMemoryGb <= availableVramGb) {
    state = requiredMemoryGb <= availableVramGb * .82 ? 'excellent' : 'good';
    mode = 'gpu';
  } else if (availableVramGb > 0 && requiredMemoryGb <= combinedAvailableGb) {
    state = requiredMemoryGb <= combinedAvailableGb * .72 ? 'good' : 'limited';
    mode = 'hybrid';
  } else if (!availableVramGb && requiredMemoryGb <= availableRamGb) {
    state = 'limited';
    mode = 'cpu';
  } else {
    state = 'too-large';
    mode = 'unavailable';
  }

  let estimatedTpsMin: number | null = null;
  let estimatedTpsMax: number | null = null;
  if (workload === 'generation' && parameters && state !== 'too-large') {
    const cpuCores = hardware?.cpu.physicalCores || Math.max(1, (hardware?.cpu.cores || 8) / 2);
    const cpuSpeed = hardware?.cpu.speedMax || hardware?.cpu.speed || 3.5;
    const cpuBase = 11 * Math.pow(8 / Math.max(parameters, .3), .78) * Math.pow(cpuCores / 8, .45) * Math.pow(cpuSpeed / 3.5, .2);
    const gpuBase = 75 * Math.pow(8 / Math.max(parameters, .3), .76) * gpuFactor(hardware?.gpu?.model, totalVramGb) * quantizationFactor(model);
    const vramShare = clamp(availableVramGb / requiredMemoryGb, 0, 1);
    const midpoint = mode === 'gpu' ? gpuBase : mode === 'hybrid' ? gpuBase * (.3 + .48 * vramShare) : cpuBase;
    estimatedTpsMin = roundTps(Math.max(.2, midpoint * .58));
    estimatedTpsMax = roundTps(Math.max(estimatedTpsMin, midpoint * 1.28));
  }

  const label = state === 'excellent' ? 'Excellent fit'
    : state === 'good' ? 'Good fit'
      : state === 'limited' ? 'Runs with limits'
        : 'Too large';
  const modeLabel = mode === 'gpu' ? 'full GPU offload' : mode === 'hybrid' ? 'split VRAM + RAM' : mode === 'cpu' ? 'CPU inference' : 'insufficient memory';
  return {
    workload,
    state,
    label,
    mode,
    requiredMemoryGb: Math.round(requiredMemoryGb * 10) / 10,
    availableVramGb: Math.round(availableVramGb * 10) / 10,
    availableRamGb: Math.round(availableRamGb * 10) / 10,
    estimatedTpsMin,
    estimatedTpsMax,
    confidence: parameters && weightGb && hardware?.gpu?.model ? 'medium' : 'low',
    reason: `${modeLabel}; estimate uses model weight, quantization, reserved OS memory and detected hardware. It is not a benchmark.`,
  };
}
