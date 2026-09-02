import type { HardwareInfo, LocalModelInfo, ModelCompatibility } from './types';

const GIB = 1024 ** 3;
const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
const round = (value: number, digits = 1) => Number(value.toFixed(digits));

interface ParameterProfile {
  totalBillions: number | null;
  activeBillions: number | null;
}

export interface ModelAdvisorOptions {
  contextTokens?: number;
}

interface BandwidthProfile {
  ratedGbps: number;
  effectiveGbps: number;
  source: 'device-profile' | 'memory-topology' | 'capacity-fallback';
}

function parameterProfile(model: LocalModelInfo): ParameterProfile {
  const text = `${model.parameters || ''} ${model.model}`;
  const totalMatch = text.match(/(?:^|[^a-z0-9])([0-9]+(?:\.[0-9]+)?)\s*[bB](?:[^a-z]|$)/i);
  const activeMatch = text.match(/(?:^|[-_.:/])A\s*([0-9]+(?:\.[0-9]+)?)\s*[bB](?:[-_.:/]|$)/i)
    || text.match(/([0-9]+(?:\.[0-9]+)?)\s*[bB][-_/.]+A\s*([0-9]+(?:\.[0-9]+)?)\s*[bB]/i);
  const totalBillions = totalMatch ? Number(totalMatch[1]) : null;
  const activeBillions = activeMatch ? Number(activeMatch[activeMatch.length - 1]) : null;
  return {
    totalBillions: totalBillions && totalBillions > 0 ? totalBillions : null,
    activeBillions: activeBillions && activeBillions > 0 ? activeBillions : null,
  };
}

function quantizationBits(model: LocalModelInfo): number {
  const value = `${model.quantization || ''} ${model.model}`.toUpperCase();
  if (/F32|FP32/.test(value)) return 32;
  if (/(?:BF16|F16|FP16)/.test(value)) return 16;
  if (/Q8|INT8/.test(value)) return 8.5;
  if (/Q6/.test(value)) return 6.6;
  if (/Q5/.test(value)) return 5.6;
  if (/Q4|INT4|MXFP4|NF4/.test(value)) return 4.7;
  if (/Q3/.test(value)) return 3.7;
  if (/Q2/.test(value)) return 2.8;
  return 5.2;
}

function quantizationEfficiency(model: LocalModelInfo): number {
  const value = `${model.quantization || ''} ${model.model}`.toUpperCase();
  if (/F32|FP32/.test(value)) return .42;
  if (/(?:BF16|F16|FP16)/.test(value)) return .66;
  if (/Q8|INT8/.test(value)) return .84;
  if (/Q6/.test(value)) return .93;
  if (/Q5/.test(value)) return .98;
  if (/Q4|INT4|MXFP4|NF4/.test(value)) return 1;
  if (/Q3/.test(value)) return .9;
  if (/Q2/.test(value)) return .76;
  return .86;
}

function gpuBandwidth(modelName = '', vramGb = 0): BandwidthProfile | null {
  const gpu = modelName.toLowerCase();
  const known: Array<[RegExp, number]> = [
    [/h100|h200/, 3350], [/a100/, vramGb >= 60 ? 2039 : 1555], [/l40s/, 864],
    [/rtx pro 6000.*blackwell/, 1792], [/rtx 6000 ada|rtx a6000/, 768],
    [/rtx 5090/, 1792], [/rtx 5080/, 960], [/rtx 5070 ti/, 896], [/rtx 5070/, 672], [/rtx 5060 ti|rtx 5060/, 448],
    [/rtx 4090|rtx 3090 ti/, 1008], [/rtx 4080 super/, 736], [/rtx 4080/, 717], [/rtx 4070 ti super/, 672],
    [/rtx 4070 ti|rtx 4070 super|rtx 4070/, 504], [/rtx 4060 ti/, 288], [/rtx 4060/, 272],
    [/rtx 3090/, 936], [/rtx 3080 ti/, 912], [/rtx 3080/, 760], [/rtx 3070|rtx 3060 ti/, 448], [/rtx 3060/, 360], [/rtx 3050/, 224],
    [/rx 7900 xtx/, 960], [/rx 7900 xt/, 800], [/rx 9070 xt|rx 9070/, 640], [/rx 7900 gre/, 576], [/rx 7800 xt/, 624],
    [/rx 7700 xt/, 432], [/rx 7600 xt|rx 7600/, 288], [/rx 6900 xt|rx 6800 xt|rx 6800/, 512], [/rx 6700 xt/, 384], [/rx 6600 xt/, 256],
    [/arc b580/, 456], [/arc b570/, 380], [/arc a770/, 560], [/arc a750|arc a580/, 512], [/arc a380/, 186],
    [/apple.*m4 ultra/, 819], [/apple.*m4 max/, 546], [/apple.*m4 pro/, 273], [/apple.*m4/, 120],
    [/apple.*m3 max/, 400], [/apple.*m3 pro/, 150], [/apple.*m3/, 100],
    [/apple.*m2 ultra|apple.*m1 ultra/, 800], [/apple.*m2 max|apple.*m1 max/, 400], [/apple.*m2 pro|apple.*m1 pro/, 200],
    [/apple.*m2/, 100], [/apple.*m1/, 68],
  ];
  const match = known.find(([pattern]) => pattern.test(gpu));
  const ratedGbps = match?.[1] || (vramGb > 0 ? clamp(105 + vramGb * 28, 140, 900) : 0);
  if (!ratedGbps) return null;
  const utilization = /apple/.test(gpu) ? .53 : /radeon|rx \d/.test(gpu) ? .46 : /arc /.test(gpu) ? .44 : .58;
  return {
    ratedGbps,
    effectiveGbps: ratedGbps * utilization,
    source: match ? 'device-profile' : 'capacity-fallback',
  };
}

function cpuBandwidth(hardware?: HardwareInfo): BandwidthProfile {
  const cpu = hardware?.cpu.brand.toLowerCase() || '';
  const apple = gpuBandwidth(hardware?.gpu?.model || hardware?.cpu.brand, (hardware?.gpu?.vramBytes || 0) / GIB);
  if (/apple|\bm[1-9]\b/.test(cpu) && apple) return apple;
  const modules = hardware?.memory.modules || [];
  const dataRate = Math.max(0, ...modules.map((module) => module.ratedClockMhz || module.clockMhz || 0));
  const populated = Math.max(1, modules.filter((module) => module.sizeBytes > 0).length);
  const channels = /epyc/.test(cpu) ? Math.min(12, populated)
    : /threadripper/.test(cpu) ? Math.min(8, populated)
      : /xeon|workstation/.test(cpu) ? Math.min(8, populated)
        : Math.min(2, populated);
  if (dataRate > 0) {
    const theoretical = dataRate * 8 * channels / 1000;
    return { ratedGbps: theoretical, effectiveGbps: theoretical * .67, source: 'memory-topology' };
  }
  const cores = hardware?.cpu.physicalCores || Math.max(1, (hardware?.cpu.cores || 8) / 2);
  const fallback = clamp(28 + cores * 2.2, 32, /epyc|threadripper|xeon/.test(cpu) ? 240 : 85);
  return { ratedGbps: fallback / .62, effectiveGbps: fallback, source: 'capacity-fallback' };
}

function isSharedMemory(hardware?: HardwareInfo): boolean {
  const platform = hardware?.os.platform.toLowerCase() || '';
  const gpu = hardware?.gpu?.model.toLowerCase() || '';
  return /darwin|macos/.test(platform)
    || /apple.*m[1-9]/.test(gpu)
    || /intel.*(?:uhd|iris|hd graphics)/.test(gpu)
    || /radeon.*(?:780m|890m|860m|graphics)/.test(gpu) && !/\brx\b/.test(gpu);
}

function storageBandwidth(hardware?: HardwareInfo): number {
  const devices = hardware?.storage || [];
  if (devices.some((disk) => /nvme|pcie/i.test(`${disk.type} ${disk.busType}`))) return 2.4;
  if (devices.some((disk) => /ssd|sata/i.test(`${disk.type} ${disk.busType}`))) return .48;
  return .12;
}

function runtimeEfficiency(model: LocalModelInfo): number {
  const runtime = `${model.application} ${model.runtime}`.toLowerCase();
  if (/llama\.cpp|ollama|lm studio/.test(runtime)) return 1;
  if (/vllm|mlx/.test(runtime)) return 1.06;
  if (/gpt4all|kobold/.test(runtime)) return .84;
  return .9;
}

function roundTps(value: number) {
  if (value >= 200) return Math.round(value / 10) * 10;
  if (value >= 50) return Math.round(value / 2) * 2;
  if (value >= 10) return Math.round(value * 2) / 2;
  if (value >= 1) return Math.round(value * 10) / 10;
  return Math.max(.01, Math.round(value * 100) / 100);
}

export function analyzeLocalModel(model: LocalModelInfo, hardware?: HardwareInfo, options: ModelAdvisorOptions = {}): ModelCompatibility {
  const workload: ModelCompatibility['workload'] = /(?:^|[-_/.:])(embed|embedding|rerank(?:er)?|bge|e5|gte)(?:[-_/.:]|$)/i.test(model.model)
    ? 'embedding'
    : 'generation';
  const totalRamGb = (hardware?.memory.totalBytes || 0) / GIB;
  const reportedVramGb = (hardware?.gpu?.vramBytes || 0) / GIB;
  const unifiedMemory = isSharedMemory(hardware);
  const totalVramGb = unifiedMemory ? 0 : reportedVramGb;
  const ramReserveGb = totalRamGb ? Math.min(Math.max(3, totalRamGb * .12), Math.max(0, totalRamGb - 2)) : 0;
  const vramReserveGb = totalVramGb ? Math.min(Math.max(.75, totalVramGb * .08), Math.max(0, totalVramGb - .5)) : 0;
  const availableRamGb = Math.max(0, totalRamGb - ramReserveGb);
  const availableVramGb = Math.max(0, totalVramGb - vramReserveGb);
  const usableCombinedMemoryGb = unifiedMemory ? availableRamGb : availableRamGb + availableVramGb;
  const totalCombinedMemoryGb = unifiedMemory ? totalRamGb : totalRamGb + totalVramGb;
  // Exact GGUF metadata wins when present; filename heuristics are fallbacks.
  const exactParameters = model.parameterCountExact && model.parameterCountExact > 0
    ? model.parameterCountExact / 1_000_000_000
    : null;
  const exactWeightGb = model.weightBytesExact && model.weightBytesExact > 0
    ? model.weightBytesExact / GIB
    : null;
  const parameters: ParameterProfile = exactParameters
    ? {
        totalBillions: exactParameters,
        // GGUF MoE keys give the true active ratio; fall back to name parsing.
        activeBillions: model.moe && model.activeParameterRatio
          ? exactParameters * model.activeParameterRatio
          : parameterProfile(model).activeBillions,
      }
    : parameterProfile(model);
  const bits = quantizationBits(model);
  const reportedWeightGb = exactWeightGb
    || (model.sizeBytes || 0) / GIB;
  const inferredWeightGb = parameters.totalBillions ? parameters.totalBillions * bits / 8 * 1.035 : 0;
  const modelWeightGb = reportedWeightGb || inferredWeightGb || (model.allocatedBytes ? model.allocatedBytes / GIB * .9 : 0);
  const requestedContext = Math.max(512, options.contextTokens || 4096);
  const assumedContextTokens = model.contextLength ? Math.min(requestedContext, model.contextLength) : requestedContext;

  if (!modelWeightGb || !totalRamGb) {
    return {
      workload,
      state: 'unknown', label: 'Needs metadata', mode: 'unavailable', requiredMemoryGb: 0,
      modelWeightGb: round(modelWeightGb), runtimeOverheadGb: 0, kvCacheGb: 0,
      totalRamGb: round(totalRamGb), totalVramGb: round(totalVramGb), totalCombinedMemoryGb: round(totalCombinedMemoryGb),
      availableVramGb: round(availableVramGb), availableRamGb: round(availableRamGb), usableCombinedMemoryGb: round(usableCombinedMemoryGb),
      ramReserveGb: round(ramReserveGb), vramReserveGb: round(vramReserveGb), isUnifiedMemory: unifiedMemory,
      assumedContextTokens, gpuOffloadPercent: 0, memoryDeficitGb: 0,
      estimatedTpsMin: null, estimatedTpsMax: null, estimatedTpsCenter: null, effectiveBandwidthGbps: null,
      confidence: 'low', confidenceScore: 20, bottleneck: 'metadata',
      reason: 'The runtime did not expose a model size, allocation or parameter count, so a defensible throughput range cannot be calculated.',
    };
  }

  const totalParameters = parameters.totalBillions || Math.max(.1, modelWeightGb * 8 / bits);
  const activeRatio = parameters.activeBillions && parameters.totalBillions
    ? clamp(.1 + .9 * parameters.activeBillions / parameters.totalBillions, .08, 1)
    : 1;
  const activeWeightGb = modelWeightGb * activeRatio;
  // Exact KV heads from the GGUF header replace the parameter-count heuristic:
  // 2 bytes/elem (K+V f16) × layers × kv_heads × head_dim × context tokens.
  const headDim = 128;
  const kvBytesPerTokenExact = model.layers && model.kvHeads
    ? 2 * model.layers * model.kvHeads * headDim * 2
    : null;
  const kvBytesPerToken = workload === 'embedding'
    ? 0
    : kvBytesPerTokenExact ?? 128 * 1024 * Math.sqrt(Math.max(.25, totalParameters) / 8);
  const kvCacheGb = kvBytesPerToken * assumedContextTokens / GIB;
  const runtimeOverheadGb = .35 + modelWeightGb * .035 + Math.min(2.5, totalParameters * .012);
  const modeledRequirement = modelWeightGb + kvCacheGb + runtimeOverheadGb;
  const observedAllocationGb = (model.allocatedBytes || 0) / GIB;
  const requiredMemoryGb = Math.max(modeledRequirement, observedAllocationGb);
  const memoryDeficitGb = Math.max(0, requiredMemoryGb - usableCombinedMemoryGb);

  let state: ModelCompatibility['state'];
  let mode: ModelCompatibility['mode'];
  if (availableVramGb > 0 && requiredMemoryGb <= availableVramGb) {
    state = requiredMemoryGb <= availableVramGb * .82 ? 'excellent' : 'good';
    mode = 'gpu';
  } else if (availableVramGb > 0 && requiredMemoryGb <= usableCombinedMemoryGb) {
    state = requiredMemoryGb <= usableCombinedMemoryGb * .82 ? 'good' : 'limited';
    mode = 'hybrid';
  } else if (!availableVramGb && requiredMemoryGb <= availableRamGb) {
    state = requiredMemoryGb <= availableRamGb * .8 ? 'good' : 'limited';
    mode = 'cpu';
  } else {
    state = 'too-large';
    mode = 'paging';
  }

  const gpuProfile = gpuBandwidth(hardware?.gpu?.model, totalVramGb);
  const cpuProfile = cpuBandwidth(hardware);
  const quantEfficiency = quantizationEfficiency(model);
  const runtimeFactor = runtimeEfficiency(model);
  const contextReadGb = kvCacheGb * .72;
  const gpuBudgetForWeights = Math.max(0, availableVramGb - kvCacheGb - runtimeOverheadGb * .5);
  // Layer/expert placement is based on the complete weight set. MoE active
  // bytes reduce per-token traffic, but do not make every expert fit in VRAM.
  const gpuCapacityShare = mode === 'gpu' ? 1 : clamp(gpuBudgetForWeights / modelWeightGb, 0, 1);
  const gpuWeightGb = activeWeightGb * gpuCapacityShare;
  const cpuWeightGb = Math.max(0, activeWeightGb - gpuWeightGb);
  const residentWeightGb = activeWeightGb * clamp(usableCombinedMemoryGb / requiredMemoryGb, 0, 1);
  const pagedWeightGb = mode === 'paging' ? Math.max(.05, activeWeightGb - residentWeightGb, memoryDeficitGb * activeRatio) : 0;
  const gpuSeconds = gpuWeightGb && gpuProfile ? (gpuWeightGb + contextReadGb * (gpuWeightGb / activeWeightGb)) / gpuProfile.effectiveGbps : 0;
  const cpuSeconds = cpuWeightGb ? (cpuWeightGb + contextReadGb * (cpuWeightGb / activeWeightGb)) / cpuProfile.effectiveGbps : 0;
  const pagingSeconds = pagedWeightGb ? pagedWeightGb / storageBandwidth(hardware) : 0;
  const transferFactor = mode === 'hybrid' ? 1.13 : mode === 'paging' ? 1.28 : 1;
  const secondsPerToken = Math.max(.0005, (gpuSeconds + cpuSeconds + pagingSeconds) * transferFactor / (quantEfficiency * runtimeFactor));
  const center = clamp(1 / secondsPerToken, .01, 2500);
  const bandwidthSourceKnown = gpuWeightGb > 0
    ? gpuProfile?.source === 'device-profile' && (cpuWeightGb === 0 || cpuProfile.source !== 'capacity-fallback')
    : cpuProfile.source !== 'capacity-fallback';
  let confidenceScore = 25;
  if (reportedWeightGb) confidenceScore += 20;
  if (parameters.totalBillions) confidenceScore += 12;
  if (model.quantization) confidenceScore += 8;
  if (exactParameters) confidenceScore += 8;
  if (kvBytesPerTokenExact) confidenceScore += 6;
  if (gpuProfile?.source === 'device-profile' || (!gpuWeightGb && cpuProfile.source === 'memory-topology')) confidenceScore += 10;
  if (model.contextLength) confidenceScore += 5;
  if (mode === 'paging') confidenceScore = Math.min(confidenceScore - 18, 48);
  confidenceScore = Math.round(clamp(confidenceScore, 20, 95));
  const uncertainty = confidenceScore >= 70 ? [.72, 1.22] : confidenceScore >= 50 ? [.58, 1.35] : [.42, 1.55];
  const estimatedTpsCenter = workload === 'generation' ? roundTps(center) : null;
  const estimatedTpsMin = workload === 'generation' ? roundTps(center * uncertainty[0]) : null;
  const estimatedTpsMax = workload === 'generation' ? roundTps(Math.max(center * uncertainty[1], center * uncertainty[0])) : null;
  const gpuOffloadPercent = gpuCapacityShare * 100;
  const effectiveBandwidthGbps = center * (activeWeightGb + contextReadGb);
  const bottleneck: ModelCompatibility['bottleneck'] = mode === 'paging' ? 'storage paging'
    : mode === 'hybrid' ? 'system RAM / split offload'
      : mode === 'cpu' ? 'system memory bandwidth'
        : 'GPU memory bandwidth';
  const label = state === 'excellent' ? 'Excellent fit'
    : state === 'good' ? 'Good fit'
      : state === 'limited' ? 'Runs with limits'
        : 'Exceeds fast memory';
  const modeLabel = mode === 'gpu' ? 'full GPU offload'
    : mode === 'hybrid' ? 'split VRAM + RAM'
      : mode === 'cpu' ? 'CPU / unified-memory inference'
        : 'conditional paging / mmap fallback';
  const sourceLabel = bandwidthSourceKnown ? 'detected memory topology' : 'hardware-class fallback';
  const metadataSource = exactParameters || exactWeightGb
    ? 'exact GGUF header metadata'
    : 'name and size heuristics';

  return {
    workload,
    state,
    label,
    mode,
    requiredMemoryGb: round(requiredMemoryGb),
    modelWeightGb: round(modelWeightGb),
    runtimeOverheadGb: round(runtimeOverheadGb),
    kvCacheGb: round(kvCacheGb, 2),
    totalRamGb: round(totalRamGb),
    totalVramGb: round(totalVramGb),
    totalCombinedMemoryGb: round(totalCombinedMemoryGb),
    availableVramGb: round(availableVramGb),
    availableRamGb: round(availableRamGb),
    usableCombinedMemoryGb: round(usableCombinedMemoryGb),
    ramReserveGb: round(ramReserveGb),
    vramReserveGb: round(vramReserveGb),
    isUnifiedMemory: unifiedMemory,
    assumedContextTokens,
    gpuOffloadPercent: round(gpuOffloadPercent),
    memoryDeficitGb: round(memoryDeficitGb),
    estimatedTpsMin,
    estimatedTpsMax,
    estimatedTpsCenter,
    effectiveBandwidthGbps: round(effectiveBandwidthGbps),
    confidence: confidenceScore >= 55 ? 'medium' : 'low',
    confidenceScore,
    bottleneck,
    reason: `${modeLabel}; ${round(gpuOffloadPercent)}% estimated GPU offload at ${assumedContextTokens.toLocaleString()} context. Memory math uses ${metadataSource}; throughput is bandwidth-modeled from ${sourceLabel}, not benchmarked.`,
  };
}
