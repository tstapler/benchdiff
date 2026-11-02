'use strict';
// scripts/check-wasm-size.sh: prints "<bytes> / <budget> bytes" and exits 0 within budget, 1 above it.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const root = path.resolve(__dirname, '..');
const script = path.join(root, 'scripts', 'check-wasm-size.sh');
const run = (...args) => spawnSync('sh', [script, ...args], { cwd: root, encoding: 'utf8' });
const tmpFile = (size) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-size-'));
  const p = path.join(dir, 'x.wasm');
  fs.writeFileSync(p, Buffer.alloc(size));
  return p;
};

test('wasm_size_should_exit_0_and_print_ratio_when_file_within_budget', () => {
  const r = run(tmpFile(330650));
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /^330650 \/ 820000 bytes\n?/);
});

test('wasm_size_should_exit_1_when_file_exceeds_budget', () => {
  const r = run(tmpFile(820001));
  assert.equal(r.status, 1);
  assert.match(r.stdout + r.stderr, /820001 \/ 820000 bytes/);
});

test('wasm_size_should_pass_at_exact_budget', () => {
  assert.equal(run(tmpFile(820000)).status, 0);
});

test('wasm_size_should_fail_when_file_missing', () => {
  assert.notEqual(run('/nonexistent/x.wasm').status, 0);
});

test('wasm_size_should_default_to_committed_dist_wasm', () => {
  const r = run();
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.match(r.stdout, /^\d+ \/ 820000 bytes/);
});
