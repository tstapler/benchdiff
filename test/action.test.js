'use strict';
// Action-level tests: argument building, input validation and an end-to-end main() run.
// Needs the committed dist/benchdiff.wasm only (no native binary).

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { buildArgs, stageConfig, parseFailOnRegression, validateTag, main } = require('../index.js');

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

const TAIL = ['--markdown', '/work/report.md', '--json', '/work/report.json', '--head-json', '/work/head.json'];

// Intentional 1.2.4a change: unset inputs no longer inject the documented defaults; the binary
// (or a --config file) owns them. The diff against the 0.1.3a snapshot is exactly the dropped
// `--format auto --threshold 5 --min-abs 0 --sort delta --title=Benchmark comparison --tag=default`.
test('buildArgs snapshot: all-empty inputs emit no config-governed flags', () => {
  const args = buildArgs({ head: ['h.txt'], base: ['b.txt'], format: '', threshold: '', minAbs: '', sort: '', title: '', tag: '', failOnRegression: 'false' });
  assert.deepEqual(args, ['--head', 'h.txt', '--base', 'b.txt', ...TAIL]);
});

test('buildArgs: explicitly set defaults are still emitted (inputs override the config file)', () => {
  const args = buildArgs({ head: ['h.txt'], base: ['b.txt'], format: 'auto', threshold: '5', minAbs: '0', sort: 'delta', title: 'Benchmark comparison', tag: 'default', failOnRegression: 'false' });
  assert.deepEqual(args, [
    '--head', 'h.txt', '--base', 'b.txt',
    '--format', 'auto', '--threshold', '5', '--min-abs', '0', '--sort', 'delta',
    '--title=Benchmark comparison', '--tag=default',
    ...TAIL,
  ]);
});

test('buildArgs: config is passed as --config and never invented', () => {
  assert.ok(!buildArgs({ ...base, config: '' }).includes('--config'));
  assert.equal(flagValue(buildArgs({ ...base, config: '/work/config/benchdiff.config.json' }), '--config'), '/work/config/benchdiff.config.json');
});

test('action.yml: config-governed inputs have no default and a config input exists', () => {
  const yml = fs.readFileSync(path.join(root, 'action.yml'), 'utf8');
  const block = (name) => yml.split(new RegExp(`^  ${name}:\\n`, 'm'))[1].split(/^  \S/m)[0];
  for (const name of ['format', 'base-format', 'threshold', 'min-abs', 'only-changed', 'sort', 'max-rows', 'title', 'tag', 'fail-on-regression', 'config']) {
    assert.ok(!/^    default:/m.test(block(name)), `${name} still has a default:`);
  }
  assert.match(block('threshold'), /default 5/i);
  assert.match(block('min-abs'), /default 0/i);
  assert.match(block('sort'), /default delta/i);
  assert.match(yml, /^  config:$/m);
});

test('stageConfig copies the config byte for byte under its own sanitized base name', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-cfg-'));
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-ws-'));
  const text = '{"version":1,\n "threshold":3}';
  fs.writeFileSync(path.join(ws, 'benchdiff.config.json'), text);
  fs.writeFileSync(path.join(ws, 'my config (1).json'), text);
  assert.equal(stageConfig('benchdiff.config.json', dir, ws), '/work/config/benchdiff.config.json');
  assert.equal(fs.readFileSync(path.join(dir, 'config', 'benchdiff.config.json'), 'utf8'), text);
  assert.equal(stageConfig('my config (1).json', dir, ws), '/work/config/my_config__1_.json');
  assert.throws(() => stageConfig('nope.json', dir, ws), /config file not found: nope\.json/);
});

test('main() output is identical with a stray benchdiff.json in the workspace and passes no --config', async () => {
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-stray-'));
  fs.writeFileSync(path.join(ws, 'benchdiff.json'), '{"schema":1,"metrics":[]}');
  const run = async (cwd) => {
    const s = setup('false');
    const before = process.cwd();
    process.chdir(cwd);
    try {
      await withEnv(s.env, () => main());
    } finally {
      process.chdir(before);
    }
    return fs.readFileSync(s.summary, 'utf8');
  };
  assert.equal(await run(ws), await run(root));
});

test('buildArgs snapshot: every input set yields the exact current argv', () => {
  const args = buildArgs({
    head: ['h1', 'h2'], base: ['b1'], format: 'go', baseFormat: 'benchdiff', threshold: '7.5', minAbs: '2',
    onlyChanged: true, sort: 'name', maxRows: '9', title: 'My title', tag: 'perf', failOnRegression: '10',
  });
  assert.deepEqual(args, [
    '--head', 'h1', '--head', 'h2', '--base', 'b1',
    '--format', 'go', '--threshold', '7.5', '--min-abs', '2', '--sort', 'name',
    '--title=My title', '--tag=perf',
    '--base-format', 'benchdiff', '--only-changed', '--max-rows', '9', '--fail-on-regression=10',
    ...TAIL,
  ]);
});

test('buildArgs snapshot: onlyChanged and failOnRegression forms', () => {
  const tail = (o) => buildArgs({ ...base, ...o }).slice(-TAIL.length - 1, -TAIL.length);
  assert.deepEqual(buildArgs({ ...base, onlyChanged: true }).filter((a) => a === '--only-changed'), ['--only-changed']);
  assert.deepEqual(buildArgs({ ...base, onlyChanged: false }).filter((a) => a === '--only-changed'), []);
  assert.deepEqual(tail({ failOnRegression: 'true' }), ['--fail-on-regression']);
  assert.deepEqual(tail({ failOnRegression: '0' }), ['--fail-on-regression=0']);
  assert.deepEqual(tail({ failOnRegression: '12.5' }), ['--fail-on-regression=12.5']);
});

test('package_json_should_be_absent_or_have_no_dependencies_and_index_and_lib_should_require_only_node_builtins', () => {
  const firstDependency = (file) => {
    if (!fs.existsSync(file)) return null;
    const deps = JSON.parse(fs.readFileSync(file, 'utf8')).dependencies;
    return deps && Object.keys(deps).length > 0 ? Object.keys(deps)[0] : null;
  };
  assert.equal(firstDependency(path.join(root, 'package.json')), null);

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-pkg-'));
  fs.writeFileSync(path.join(tmp, 'package.json'), JSON.stringify({ dependencies: { left_pad: '1.0.0', other: '2.0.0' } }));
  assert.equal(firstDependency(path.join(tmp, 'package.json')), 'left_pad');

  const libDir = path.join(root, 'lib');
  const libFiles = fs.existsSync(libDir) ? fs.readdirSync(libDir).filter((f) => f.endsWith('.js')).map((f) => path.join(libDir, f)) : [];
  for (const file of [path.join(root, 'index.js'), ...libFiles]) {
    if (!fs.existsSync(file)) continue;
    const src = fs.readFileSync(file, 'utf8');
    const specs = [...src.matchAll(/\brequire\(\s*['"]([^'"]+)['"]\s*\)/g), ...src.matchAll(/^\s*import\s[^'"\n]*['"]([^'"]+)['"]/gm)].map((m) => m[1]);
    for (const spec of specs) {
      assert.ok(spec.startsWith('node:') || spec.startsWith('./') || spec.startsWith('../'), `${path.basename(file)} requires non-builtin "${spec}"`);
    }
  }
});
