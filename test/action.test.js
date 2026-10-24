'use strict';
// Action-level tests: argument building, input validation and an end-to-end main() run.
// Needs the committed dist/benchdiff.wasm only (no native binary).

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { buildArgs, parseFailOnRegression, validateTag, main } = require('../index.js');

const root = path.resolve(__dirname, '..');
const base = { head: ['/work/h'], base: ['/work/b'], format: 'auto', threshold: '5', minAbs: '0', sort: 'delta', title: 'T', tag: 'default', failOnRegression: 'false' };
const flagValue = (args, flag) => args[args.indexOf(flag) + 1];

test('buildArgs: fail-on-regression true is a bare flag, a number is =N, false/empty add nothing', () => {
  assert.ok(buildArgs({ ...base, failOnRegression: 'true' }).includes('--fail-on-regression'));
  assert.ok(buildArgs({ ...base, failOnRegression: '10' }).includes('--fail-on-regression=10'));
  for (const v of ['false', '']) assert.ok(!buildArgs({ ...base, failOnRegression: v }).some((a) => a.startsWith('--fail-on-regression')));
});

test('buildArgs: only-changed, max-rows and base-format pass through', () => {
  const none = buildArgs(base);
  for (const f of ['--only-changed', '--max-rows', '--base-format']) assert.ok(!none.includes(f));
  const args = buildArgs({ ...base, onlyChanged: true, maxRows: '3', baseFormat: 'benchdiff' });
  assert.ok(args.includes('--only-changed'));
  assert.equal(flagValue(args, '--max-rows'), '3');
  assert.equal(flagValue(args, '--base-format'), 'benchdiff');
});

test('parseFailOnRegression normalizes case/space and rejects junk', () => {
  assert.equal(parseFailOnRegression(' TRUE '), 'true');
  assert.equal(parseFailOnRegression('False'), 'false');
  assert.equal(parseFailOnRegression('12.5'), '12.5');
  for (const bad of ['abc', '-1', 'NaN', 'Infinity']) assert.throws(() => parseFailOnRegression(bad), /fail-on-regression/);
});

test('validateTag rejects anything the Rust marker() would rewrite', () => {
  assert.equal(validateTag('perf-1'), 'perf-1');
  for (const bad of ['a-->b', 'a\nb', 'a\u0000b', 'my tag', 'a<b']) assert.throws(() => validateTag(bad), /tag/);
});

function withEnv(vars, fn) {
  const saved = { ...process.env };
  Object.assign(process.env, vars);
  return Promise.resolve(fn()).finally(() => {
    for (const k of Object.keys(process.env)) if (!(k in saved)) delete process.env[k];
    Object.assign(process.env, saved);
  });
}

function setup(limit, head = path.join(root, 'testdata/golden/head.go.txt')) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-main-'));
  const output = path.join(dir, 'output');
  const summary = path.join(dir, 'summary');
  const env = {
    INPUT_HEAD: head,
    INPUT_BASE: path.join(root, 'testdata/golden/base.go.txt'),
    'INPUT_FAIL-ON-REGRESSION': limit,
    INPUT_COMMENT: 'false',
    GITHUB_OUTPUT: output,
    GITHUB_STEP_SUMMARY: summary,
    RUNNER_TEMP: dir,
  };
  return { env, output, summary };
}

test('main() fails on a regression over the limit and publishes outputs and summary', async () => {
  const s = setup('10');
  await withEnv(s.env, async () => {
    await assert.rejects(main(), /regression/);
  });
  const out = fs.readFileSync(s.output, 'utf8');
  assert.match(out, /^regressed=1$/m);
  assert.match(out, /^improved=1$/m);
  assert.match(out, /^failed=true$/m);
  assert.ok(fs.readFileSync(s.summary, 'utf8').startsWith('<!-- benchdiff:default -->'));
});

test('main() passes when the regression is under the limit', async () => {
  const s = setup('50');
  await withEnv(s.env, () => main());
  assert.match(fs.readFileSync(s.output, 'utf8'), /^failed=false$/m);
});

test('main() rejects when no head file matches', async () => {
  const s = setup('false', path.join(os.tmpdir(), 'bd-does-not-exist.txt'));
  await withEnv(s.env, async () => {
    await assert.rejects(main(), /no head files match/);
  });
});

test('an accepted tag yields the same marker in the wasm report as in index.js', async () => {
  const { runWasm, stage } = require('../index.js');
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-marker-'));
  const head = stage([path.join(__dirname, '..', 'testdata', 'go', 'bench.txt')], dir, 'head');
  const tag = validateTag('rel/1.0:x_y-z');
  const r = await runWasm(['--head', head[0], '--tag', tag, '--markdown', '/work/report.md'], dir);
  assert.equal(r.code, 0, r.stderr);
  assert.equal(fs.readFileSync(path.join(dir, 'report.md'), 'utf8').split('\n', 1)[0], `<!-- benchdiff:${tag} -->`);
});

test('buildArgs passes free-text values inline so a title starting with -- is not read as a flag', () => {
  const args = buildArgs({ ...base, title: '--- x ---' });
  assert.ok(args.includes('--title=--- x ---'));
});

test('parseFailOnRegression rejects hex and other non-decimal numbers', () => {
  for (const bad of ['0x10', '1e3', '-1', 'abc']) assert.throws(() => parseFailOnRegression(bad), /fail-on-regression/);
  assert.equal(parseFailOnRegression('12.5'), '12.5');
});
