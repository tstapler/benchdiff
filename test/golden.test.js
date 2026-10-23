'use strict';
// Golden test: the committed dist/benchdiff.wasm, driven through index.js's WASI shim, must
// produce byte-identical output to the native binary. A stale wasm fails here.
// Needs a native binary: `cargo build --release -p benchdiff` (or set BENCHDIFF_NATIVE).

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { runWasm, stage, expand, upsertComment, clip } = require('../index.js');

const root = path.resolve(__dirname, '..');
const native = process.env.BENCHDIFF_NATIVE || path.join(root, 'target', 'release', 'benchdiff');
const td = (...p) => path.join(root, 'testdata', ...p);

const cases = {
  'go base vs head': { head: [td('golden/head.go.txt')], base: [td('golden/base.go.txt')], extra: ['--title', 'Go'] },
  'go with only-changed, max-rows, sort name': { head: [td('golden/head.go.txt')], base: [td('golden/base.go.txt')], extra: ['--only-changed', '--max-rows', '1', '--sort', 'name'] },
  'criterion head only': { head: [td('criterion/criterion.jsonl')], base: [], extra: [] },
  'cargo bench head only': { head: [td('cargo-bench/bench.txt')], base: [], extra: ['--tag', 'rust'] },
  'jmh vs itself with threshold 0 and no stats': { head: [td('jmh/jmh.json')], base: [td('jmh/jmh.json')], extra: ['--threshold', '0', '--no-stats'] },
  'merged heads (bmf + gab)': { head: [td('bmf/bmf.json'), td('gab/custom.json')], base: [], extra: ['--format', 'auto'] },
  'fail-on-regression trips': { head: [td('golden/head.go.txt')], base: [td('golden/base.go.txt')], extra: ['--fail-on-regression=10'], exit: 1 },
};

function runNative(args, dir) {
  const r = spawnSync(native, args, { cwd: dir, encoding: 'utf8' });
  assert.notEqual(r.status, null, `native binary missing or crashed: ${r.error}`);
  return r.status;
}

for (const [name, c] of Object.entries(cases)) {
  test(`wasm matches native: ${name}`, async () => {
    const wasmDir = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-wasm-'));
    const nativeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-native-'));
    const outputs = ['--markdown', 'report.md', '--json', 'report.json', '--head-json', 'head.json'];

    const wasmArgs = [
      ...stage(c.head, wasmDir, 'head').flatMap((f) => ['--head', f]),
      ...stage(c.base, wasmDir, 'base').flatMap((f) => ['--base', f]),
      ...c.extra,
      ...outputs.map((o) => (o.startsWith('--') ? o : `/work/${o}`)),
    ];
    const nativeArgs = [
      ...stage(c.head, nativeDir, 'head').flatMap((f) => ['--head', f.replace('/work', nativeDir)]),
      ...stage(c.base, nativeDir, 'base').flatMap((f) => ['--base', f.replace('/work', nativeDir)]),
      ...c.extra,
      ...outputs.map((o) => (o.startsWith('--') ? o : path.join(nativeDir, o))),
    ];

    const wasm = await runWasm(wasmArgs, wasmDir);
    const nativeCode = runNative(nativeArgs, nativeDir);
    assert.equal(wasm.code, c.exit ?? 0, wasm.stderr);
    assert.equal(nativeCode, c.exit ?? 0);
    for (const file of ['report.md', 'report.json', 'head.json']) {
      assert.equal(
        fs.readFileSync(path.join(wasmDir, file), 'utf8'),
        fs.readFileSync(path.join(nativeDir, file), 'utf8'),
        `${file} differs between wasm and native; rebuild dist/benchdiff.wasm (make wasm)`,
      );
    }
  });
}

test('wasm reports a usage error with exit code 2 and a message', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-wasm-'));
  const r = await runWasm(['--head', '/work/missing.json'], dir);
  assert.equal(r.code, 2);
  assert.match(r.stderr, /missing\.json/);
});

test('expand resolves literal paths and globs once each, sorted', () => {
  const files = expand(`${td('go/bench.txt')}\ntestdata/go/*.txt, testdata/go/bench.txt`, root);
  assert.deepEqual(files.map((f) => path.basename(f)), ['bench.txt', 'bench-count8.txt']);
});

function fakeFetch(routes) {
  const calls = [];
  const impl = async (url, init = {}) => {
    const key = `${init.method || 'GET'} ${new URL(url).pathname}`;
    calls.push({ key, body: init.body });
    const r = routes[key];
    if (!r) throw new Error(`unexpected request ${key}`);
    return { ok: r.status < 300, status: r.status, json: async () => r.json };
  };
  return { impl, calls };
}

const env = { GITHUB_REPOSITORY: 'o/r', GITHUB_API_URL: 'https://api.test' };
const marker = '<!-- benchdiff:t -->';

test('comment is created when no sticky comment exists', async () => {
  const f = fakeFetch({
    'GET /repos/o/r/issues/7/comments': { status: 200, json: [{ id: 1, body: 'unrelated' }] },
    'POST /repos/o/r/issues/7/comments': { status: 201, json: { id: 99 } },
  });
  const r = await upsertComment({ token: 't', marker, body: `${marker}\nhi`, fetchImpl: f.impl, env, number: 7 });
  assert.deepEqual(r, { status: 'created', id: 99 });
});

test('comment carrying the marker is updated in place, not duplicated', async () => {
  const f = fakeFetch({
    'GET /repos/o/r/issues/7/comments': { status: 200, json: [{ id: 5, body: `old\n${marker}` }] },
    'PATCH /repos/o/r/issues/comments/5': { status: 200, json: { id: 5 } },
  });
  const r = await upsertComment({ token: 't', marker, body: `${marker}\nnew`, fetchImpl: f.impl, env, number: 7 });
  assert.deepEqual(r, { status: 'updated', id: 5 });
  assert.equal(f.calls.filter((c) => c.key.startsWith('POST')).length, 0);
});

test('a read-only token (fork PR) degrades to a result the caller turns into a warning', async () => {
  const f = fakeFetch({ 'GET /repos/o/r/issues/7/comments': { status: 403, json: {} } });
  const r = await upsertComment({ token: 't', marker, body: marker, fetchImpl: f.impl, env, number: 7 });
  assert.equal(r.status, 'failed');
  assert.equal(r.readOnly, true);
});

test('outside a pull request the comment is skipped without any request', async () => {
  const f = fakeFetch({});
  const r = await upsertComment({ token: 't', marker, body: marker, fetchImpl: f.impl, env, number: undefined });
  assert.equal(r.status, 'skipped');
  assert.equal(f.calls.length, 0);
});

test('oversized reports are clipped under the comment limit and keep the marker', () => {
  const body = `${marker}\n${'row\n'.repeat(30000)}`;
  const out = clip(body, marker);
  assert.ok(out.length < 65536);
  assert.ok(out.includes(marker));
});
