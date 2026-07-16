import test from 'node:test';
import assert from 'node:assert/strict';
import { createAlertEngine } from '../agent/alert-engine.mjs';

const rule = { id: 'cpu', title: 'CPU', metric: 'cpu', threshold: 90, durationMs: 1000, unit: '%', suppressDuringGaming: true, enabled: true, severity: 'warning' };

test('sustained load only alerts after its duration', () => {
  const engine = createAlertEngine([rule]);
  assert.equal(engine.evaluate({ cpu: 96 }, { active: false }, 1000).active.length, 0);
  assert.equal(engine.evaluate({ cpu: 96 }, { active: false }, 1999).active.length, 0);
  assert.equal(engine.evaluate({ cpu: 96 }, { active: false }, 2000).active.length, 1);
});

test('gaming suppresses load alerts but not the alert engine', () => {
  const engine = createAlertEngine([rule]);
  const result = engine.evaluate({ cpu: 99 }, { active: true, processName: 'Game.exe', pid: 7 }, 1000);
  assert.equal(result.active.length, 0);
  assert.equal(result.watch[0].suppressed, true);
  assert.equal(result.gaming.processName, 'Game.exe');
});

test('resolved alerts are retained as recent evidence', () => {
  const engine = createAlertEngine([rule]);
  engine.evaluate({ cpu: 99 }, { active: false }, 1000);
  engine.evaluate({ cpu: 99 }, { active: false }, 2000);
  const result = engine.evaluate({ cpu: 20 }, { active: false }, 2500);
  assert.equal(result.recent.length, 1);
  assert.equal(result.recent[0].peak, 99);
});

test('disabled rules stop tracking immediately', () => {
  const engine = createAlertEngine([rule]);
  engine.evaluate({ cpu: 99 }, { active: false }, 1000);
  assert.equal(engine.setEnabled('cpu', false), true);
  assert.equal(engine.evaluate({ cpu: 99 }, { active: false }, 3000).active.length, 0);
});
