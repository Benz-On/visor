import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import ts from 'typescript';

const source = await readFile(new URL('../src/modelAdvisor.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ES2022, target: ts.ScriptTarget.ES2022 },
}).outputText;
const { analyzeLocalModel } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);

const GIB = 1024 ** 3;

function hardware({ ram = 32, vram = 12, gpu = 'NVIDIA GeForce RTX 4070', platform = 'windows', clock = 6000 } = {}) {
  return {
    system: { manufacturer: 'Test', model: 'Rig', version: '1' },
    os: { platform, distro: platform, release: '1', build: '1', arch: 'x64', hostname: 'test' },
    cpu: { manufacturer: 'AMD', brand: 'AMD Ryzen 9 7900X', cores: 24, physicalCores: 12, speed: 4.7, speedMax: 5.6, estimatedTdp: 170 },
    gpu: gpu ? { vendor: 'Test', model: gpu, vramBytes: vram * GIB, driverVersion: '1', powerLimit: 300 } : null,
    memory: { totalBytes: ram * GIB, modules: [{ sizeBytes: ram / 2 * GIB, type: 'DDR5', clockMhz: clock }, { sizeBytes: ram / 2 * GIB, type: 'DDR5', clockMhz: clock }] },
    storage: [{ name: 'NVMe', type: 'NVMe SSD', sizeBytes: 1_000_000_000_000, smartStatus: 'OK', busType: 'NVMe' }],
    displays: [],
  };
}

function model({ name = 'llama-3.1-8B-Q4_K_M.gguf', parameters = '8B', size = 4.6, context = 131072 } = {}) {
  return {
    id: name, application: 'Ollama', runtime: 'llama.cpp', model: name, status: 'detected', source: 'test', confidence: 95,
    family: 'llama', parameters, quantization: 'Q4_K_M', format: 'gguf', contextLength: context,
    sizeBytes: size * GIB, allocatedBytes: 0, allocatedVramBytes: 0, allocatedRamBytes: 0, allocatedVramGb: 0,
    expiresAt: null, process: null, applicationEnergyWatts: null, applicationCpu: 0, applicationGpu: 0,
  };
}

test('combines dedicated RAM and VRAM while preserving OS and display reserves', () => {
  const result = analyzeLocalModel(model(), hardware());
  assert.equal(result.totalCombinedMemoryGb, 44);
  assert.ok(result.usableCombinedMemoryGb < result.totalCombinedMemoryGb);
  assert.equal(result.mode, 'gpu');
  assert.equal(result.gpuOffloadPercent, 100);
  assert.ok(result.estimatedTpsMin > 0);
  assert.ok(result.estimatedTpsMax >= result.estimatedTpsMin);
});

test('still calculates a conditional speed when the model exceeds RAM plus VRAM', () => {
  const result = analyzeLocalModel(model({ name: 'llama-3.1-70B-Q4_K_M.gguf', parameters: '70B', size: 40 }), hardware({ ram: 16, vram: 8 }));
  assert.equal(result.state, 'too-large');
  assert.equal(result.mode, 'paging');
  assert.ok(result.memoryDeficitGb > 0);
  assert.ok(result.estimatedTpsMin > 0);
  assert.ok(result.estimatedTpsMax > result.estimatedTpsMin);
  assert.equal(result.bottleneck, 'storage paging');
});

test('uses active MoE parameters for token throughput but full weights for capacity', () => {
  const dense = analyzeLocalModel(model({ name: 'dense-30B-Q4_K_M.gguf', parameters: '30B', size: 18 }), hardware({ ram: 64, vram: 12, gpu: 'NVIDIA GeForce RTX 4090' }));
  const moe = analyzeLocalModel(model({ name: 'qwen-30B-A3B-Q4_K_M.gguf', parameters: '30B-A3B', size: 18 }), hardware({ ram: 64, vram: 12, gpu: 'NVIDIA GeForce RTX 4090' }));
  assert.equal(moe.requiredMemoryGb, dense.requiredMemoryGb);
  assert.ok(moe.estimatedTpsCenter > dense.estimatedTpsCenter);
  assert.ok(moe.gpuOffloadPercent < 100);
});

test('long context increases KV memory and lowers generation throughput', () => {
  const short = analyzeLocalModel(model(), hardware(), { contextTokens: 2048 });
  const long = analyzeLocalModel(model(), hardware(), { contextTokens: 32768 });
  assert.ok(long.kvCacheGb > short.kvCacheGb);
  assert.ok(long.requiredMemoryGb > short.requiredMemoryGb);
  assert.ok(long.estimatedTpsCenter < short.estimatedTpsCenter);
});

test('does not double count unified Apple memory as separate VRAM', () => {
  const result = analyzeLocalModel(model(), hardware({ ram: 32, vram: 24, gpu: 'Apple M4 Max', platform: 'darwin' }));
  assert.equal(result.isUnifiedMemory, true);
  assert.equal(result.totalCombinedMemoryGb, 32);
  assert.equal(result.totalVramGb, 0);
});

test('classifies embedding and reranker tags as non-generative workloads', () => {
  for (const name of ['Qwen3-Reranker-8B:Q4_K_M', 'qwen3-embedding:8b']) {
    const result = analyzeLocalModel(model({ name, parameters: '8B', size: 4.7 }), hardware());
    assert.equal(result.workload, 'embedding');
    assert.equal(result.estimatedTpsCenter, null);
  }
});

test('exact GGUF metadata raises confidence and drives the memory equation', () => {
  const exact = analyzeLocalModel({
    ...model(),
    parameterCountExact: 9_653_104_368,
    weightBytesExact: Math.round(6.1 * GIB),
    layers: 32,
    kvHeads: 8,
    kvHeadCountExact: true,
  }, hardware());
  assert.ok(exact.confidenceScore >= 70);
  assert.ok(exact.modelWeightGb > 6 && exact.modelWeightGb < 7);
  assert.ok(exact.reason.includes('exact GGUF header metadata'));
});

test('offload plan answers how many layers fit on the GPU', () => {
  const tight = analyzeLocalModel({
    ...model({ name: 'qwen3.6-35B-A3B-Q4_K_M.gguf', parameters: '35B', size: 20, context: 4096 }),
    parameterCountExact: 35_000_000_000,
    weightBytesExact: Math.round(20 * GIB),
    layers: 64,
    kvHeads: 4,
    kvHeadCountExact: true,
  }, hardware({ ram: 32, vram: 8 }));
  assert.ok(Array.isArray(tight.offloadPlan) && tight.offloadPlan.length === 4);
  const full = tight.offloadPlan[0];
  const partial = tight.offloadPlan[1];
  assert.equal(full.layersOnGpu, 64);
  assert.equal(full.fitsVram, false);
  assert.equal(partial.layersOnGpu, 48);
  assert.ok(partial.vramNeededGb < full.vramNeededGb);
  assert.ok(partial.estimatedTpsCenter !== null);
  // A partial plan should predict faster tokens than the paging verdict.
  assert.ok(tight.estimatedTpsCenter === null || partial.estimatedTpsCenter >= tight.estimatedTpsCenter);
});

test('offload plan is absent without exact layer metadata', () => {
  const heuristic = analyzeLocalModel(model(), hardware());
  assert.equal(heuristic.offloadPlan, undefined);
});

test('offload plan charges KV cache per layer, not per two layers', () => {
  // 36 layers x 8 KV heads x (128+128) x 2 bytes = 147456 B/token;
  // at 4096 context that is exactly 0.5625 GiB for the whole cache.
  const fits = analyzeLocalModel({
    ...model({ name: 'qwen3-8B-Q4_K_M.gguf', parameters: '8B', size: 0, context: 4096 }),
    weightBytesExact: Math.round(4.6 * GIB),
    layers: 36,
    kvHeads: 8,
    kvHeadCountExact: true,
  }, hardware({ ram: 32, vram: 8 }));

  assert.ok(Math.abs(fits.kvCacheGb - 0.56) < 0.02, `kvCacheGb was ${fits.kvCacheGb}`);
  const full = fits.offloadPlan[0];
  assert.equal(full.layersOnGpu, 36);
  // 4.6 GiB weights + 0.56 GiB KV + half of the runtime overhead, inside 7.25
  // GiB of usable VRAM. The old per-layer divisor asked for ~15 GiB here and
  // reported OVER for every single step.
  assert.ok(Math.abs(full.vramNeededGb - 5.47) < 0.05, `vramNeededGb was ${full.vramNeededGb}`);
  assert.equal(full.fitsVram, true);
});

test('exact head dimensions drive the KV equation', () => {
  const base = {
    ...model({ name: 'test-model.gguf', parameters: '8B', size: 0, context: 4096 }),
    weightBytesExact: Math.round(4.6 * GIB),
    layers: 36,
    kvHeads: 8,
    kvHeadCountExact: true,
  };
  const assumed = analyzeLocalModel(base, hardware({ ram: 32, vram: 24 }));
  // Gemma-class heads are 256 wide, doubling KV traffic against the 128 default.
  const wide = analyzeLocalModel({ ...base, kvKeyLength: 256, kvValueLength: 256 }, hardware({ ram: 32, vram: 24 }));
  // 0.5625 -> 1.125 GiB exactly, but the reported figures are rounded to 2dp.
  assert.ok(Math.abs(wide.kvCacheGb / assumed.kvCacheGb - 2) < 0.05, `${assumed.kvCacheGb} -> ${wide.kvCacheGb}`);
  assert.ok(wide.confidenceScore > assumed.confidenceScore, 'exact head dims should raise confidence');
});

test('known weight bytes are not penalised twice for their format', () => {
  const rig = hardware({ ram: 64, vram: 24, gpu: 'NVIDIA GeForce RTX 4090' });
  const shared = {
    ...model({ name: 'test-8B-F16.gguf', parameters: '8B', size: 0, quantization: 'F16' }),
    quantization: 'F16',
    layers: 32,
    kvHeads: 8,
  };
  const measuredBytes = analyzeLocalModel({ ...shared, weightBytesExact: Math.round(16 * GIB) }, rig);
  // Without real byte counts the size is inferred from parameters x bits, and
  // only then may the per-format factor stand in for what is streamed.
  const inferredBytes = analyzeLocalModel({ ...shared, weightBytesExact: null }, rig);
  assert.ok(measuredBytes.estimatedTpsCenter > inferredBytes.estimatedTpsCenter * 1.35,
    `measured ${measuredBytes.estimatedTpsCenter} vs inferred ${inferredBytes.estimatedTpsCenter}`);
});
