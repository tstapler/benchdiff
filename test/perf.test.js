'use strict';
// Wasm perf guard: the N-way SLO input (10 variants x 300 benchmarks, 10 samples each) through
// dist/benchdiff.wasm via the WASI shim. Target < 1 s; fails only above 2 s (CI noise), warns between.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { runWasm } = require('../index.js');

const TARGET_MS = 1000;
const FAIL_MS = 2000;

/** Deterministic Go-format file for one variant (LCG, no Math.random). */
function variantText(v, benchmarks) {
  let seed = 1000 + v;
  const unit = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
  const lines = ['goos: linux', 'goarch: amd64', 'pkg: perf'];
  for (let b = 0; b < benchmarks; b++) {
    for (let s = 0; s < 10; s++) {
      const ns = (1000 + (b % 97) * 50) * (1 + v * 0.02) * (0.97 + 0.06 * unit());
      lines.push(`Benchmark_a${Math.floor(b / 10)}_s${b % 10}-8 \t 1000 \t ${ns.toFixed(2)} ns/op`);
    }
  }
  return lines.join('\n') + '\n';
}

function stageVariants(dir, variants, benchmarks) {
  const args = [];
  for (let v = 0; v < variants; v++) {
    const label = `v${String(v).padStart(2, '0')}`;
    fs.writeFileSync(path.join(dir, `${label}.txt`), variantText(v, benchmarks));
    args.push('--variant', `${label}=/work/${label}.txt`);
  }
  return args;
}

test('wasm startup: --help answers well under the budget', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-perf-help-'));
  const t0 = performance.now();
  const r = await runWasm(['--help'], dir);
  const ms = performance.now() - t0;
  console.log(`perf wasm startup (--help): ${ms.toFixed(0)} ms`);
  assert.equal(r.code, 0, r.stderr);
  assert.ok(ms < FAIL_MS, `startup took ${ms.toFixed(0)} ms (fail above ${FAIL_MS} ms)`);
});

test('wasm N-way 10 variants x 300 benchmarks with charts finishes within the budget', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-perf-nway-'));
  const variants = stageVariants(dir, 10, 300);
  const args = [
    ...variants, '--reference', 'v00',
    '--charts', 'delta,ranking,distribution', '--svg-dir', '/work/svg',
    '--markdown', '/work/report.md', '--json', '/work/report.json',
  ];
  fs.mkdirSync(path.join(dir, 'svg'));

  const t0 = performance.now();
  const r = await runWasm(args, dir);
  const ms = performance.now() - t0;

  console.log(`perf wasm n-way 10x300 + charts: ${ms.toFixed(0)} ms (target ${TARGET_MS} ms, fail above ${FAIL_MS} ms)`);
  if (ms >= TARGET_MS) console.warn(`WARNING: wasm n-way run ${ms.toFixed(0)} ms exceeds the ${TARGET_MS} ms target`);
  assert.equal(r.code, 0, r.stderr);
  assert.ok(fs.statSync(path.join(dir, 'report.md')).size > 0, 'report written');
  assert.ok(fs.readdirSync(path.join(dir, 'svg')).length > 0, 'at least one chart written');
  assert.ok(ms < FAIL_MS, `took ${ms.toFixed(0)} ms (fail above ${FAIL_MS} ms)`);
});
