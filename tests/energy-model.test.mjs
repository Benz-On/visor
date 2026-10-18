import test from 'node:test';
import assert from 'node:assert/strict';
import { attributeProcessEnergy, estimateEnergy, inferCpuTdp, projectEnergy } from '../agent/energy-model.mjs';

test('measured GPU power produces a hybrid estimate', () => {
  const result = estimateEnergy({
    cpuLoad: 50,
    gpuLoad: 80,
    cpuTdp: 170,
    cpuSpeedRatio: 1,
    measuredGpuPower: 230,
    gpuPowerLimit: 300,
    memoryTotalGb: 32,
    diskActivity: 20,
  });

  assert.equal(result.confidence, 'hybrid');
  assert.equal(result.breakdown.gpu, 230);
  assert.ok(result.watts > 300);
});

test('estimated power remains finite with missing sensors', () => {
  const result = estimateEnergy({ cpuLoad: 10, gpuLoad: 0 });
  assert.equal(result.confidence, 'estimated');
  assert.ok(Number.isFinite(result.watts));
  assert.ok(result.watts > 0);
});

test('a measured package replaces the TDP curve and lifts confidence', () => {
  const inputs = { cpuLoad: 50, gpuLoad: 60, cpuTdp: 170, cpuSpeedRatio: 1, memoryTotalGb: 32, diskActivity: 10 };
  const modeled = estimateEnergy(inputs);
  const measured = estimateEnergy({ ...inputs, measuredCpuPower: 58.2 });
  const both = estimateEnergy({ ...inputs, measuredCpuPower: 58.2, measuredGpuPower: 142 });

  assert.equal(measured.confidence, 'hybrid');
  assert.equal(measured.breakdown.cpu, 58.2);
  assert.ok(measured.breakdown.cpu < modeled.breakdown.cpu, '50% load on a 170 W part');
  // Both rails instrumented must not claim the same confidence as one.
  assert.equal(both.confidence, 'measured');
  assert.ok(both.confidenceScore > measured.confidenceScore);
  assert.match(both.methodology, /CPU package and GPU measured/);
});

test('process energy allocation favors the active workload', () => {
  const energy = estimateEnergy({ cpuLoad: 70, gpuLoad: 80, measuredGpuPower: 200 });
  const attributed = attributeProcessEnergy([
    { id: 1, cpu: 60, gpu: 70, memory: 8 },
    { id: 2, cpu: 5, gpu: 2, memory: 1 },
  ], energy, 16);

  assert.ok(attributed[0].energyWatts > attributed[1].energyWatts);
});

test('energy projections use configured tariff', () => {
  const projection = projectEnergy(500, 12.5, 0.25, 50);
  assert.equal(projection.dailyKwh, 12);
  assert.equal(projection.dailyCost, 3);
  assert.equal(projection.dailyCarbonGrams, 600);
});

test('CPU TDP heuristic scales with core count', () => {
  assert.equal(inferCpuTdp({ physicalCores: 4 }), 65);
  assert.equal(inferCpuTdp({ physicalCores: 16 }), 170);
  assert.equal(inferCpuTdp({ physicalCores: 24 }), 230);
});

test('mobile model suffix sets the power class for Intel and AMD', () => {
  assert.equal(inferCpuTdp({ physicalCores: 8, brand: 'Intel(R) Core(TM) Ultra 7 155U' }), 28);
  assert.equal(inferCpuTdp({ physicalCores: 8, brand: '13th Gen Intel(R) Core(TM) i7-1355U' }), 28);
  assert.equal(inferCpuTdp({ physicalCores: 8, brand: 'AMD Ryzen 7 6800U' }), 28);
  assert.equal(inferCpuTdp({ physicalCores: 8, brand: 'AMD Ryzen 9 7945HS' }), 55);
  assert.equal(inferCpuTdp({ physicalCores: 10, brand: 'Intel(R) Core(TM) i9-14900HX' }), 55);
  assert.equal(inferCpuTdp({ physicalCores: 6, brand: 'Intel(R) Core(TM) i5-8400' }), 105);
  assert.equal(inferCpuTdp({ physicalCores: 16, brand: 'Intel(R) Core(TM) 13th Gen Ultra 9' }), 170);
});
