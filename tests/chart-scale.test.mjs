import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import ts from 'typescript';

const source = await readFile(new URL('../src/chartScale.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ES2022, target: ts.ScriptTarget.ES2022 },
}).outputText;
const { niceCeiling } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);

test('percentage charts keep the 0-100 scale', () => {
  assert.equal(niceCeiling([]), 100);
  assert.equal(niceCeiling([0, 0]), 100);
  assert.equal(niceCeiling([12, 47, 88]), 100);
});

test('token rates are never clipped at one hundred', () => {
  // The defect this replaces: a 137 tok/s decode series drawn against a fixed
  // 0-100 axis became a flat ceiling line while the panel read "Peak 137".
  assert.equal(niceCeiling([120, 137, 96]), 200);
  assert.equal(niceCeiling([480]), 500);
  // Ceilings snap to 1/2/5x10^n, so 512 rides the next band up rather than
  // hugging the line; containment is what matters, tightness is aesthetic.
  assert.equal(niceCeiling([480, 512]), 1_000);
  assert.equal(niceCeiling([1_900]), 2_000);
});

test('the ceiling always contains the data', () => {
  for (const peak of [1, 3, 7, 19, 61, 100, 101, 199, 200, 499, 1_024, 33_500, 250_000]) {
    assert.ok(niceCeiling([peak]) >= peak, `${peak} -> ${niceCeiling([peak])}`);
    // And stays within one "nice" band, so a small series is not shrunk away.
    assert.ok(niceCeiling([peak]) <= peak * 10, `${peak} -> ${niceCeiling([peak])}`);
  }
});

test('negative and non-finite readings do not widen the scale', () => {
  assert.equal(niceCeiling([-40, -3]), 100);
  // An unusable reading is skipped; the scale still comes from real data.
  assert.equal(niceCeiling([NaN, 40]), 50);
  assert.equal(niceCeiling([Infinity, 40]), 50);
  assert.ok(Number.isFinite(niceCeiling([Infinity, 40])));
});
