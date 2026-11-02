'use strict';
// Action-level tests: argument building, input validation and an end-to-end main() run.
// Needs the committed dist/benchdiff.wasm only (no native binary).

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { buildArgs, stageConfig, parseFailOnRegression, validateTag, clip, main } = require('../index.js');

const root = path.resolve(__dirname, '..');
const base = { head: ['/work/h'], base: ['/work/b'], format: 'auto', threshold: '5', minAbs: '0', sort: 'delta', title: 'T', tag: 'default', failOnRegression: '' };
const flagValue = (args, flag) => args[args.indexOf(flag) + 1];

test('buildArgs: fail-on-regression true is a bare flag, a number is =N, false/empty add nothing', () => {
  assert.ok(buildArgs({ ...base, failOnRegression: 'true' }).includes('--fail-on-regression'));
  assert.ok(buildArgs({ ...base, failOnRegression: '10' }).includes('--fail-on-regression=10'));
  assert.ok(!buildArgs({ ...base, failOnRegression: '' }).some((a) => a.includes('fail-on-regression')));
});

test('buildArgs: explicit false emits the negative flags so it overrides a config file; empty emits nothing', () => {
  const fail = (v) => buildArgs({ ...base, failOnRegression: v }).filter((a) => a.includes('fail-on-regression'));
  assert.deepEqual(fail('false'), ['--no-fail-on-regression']);
  assert.deepEqual(fail(''), []);
  const only = (v) => buildArgs({ ...base, onlyChanged: v }).filter((a) => a.includes('only-changed'));
  assert.deepEqual(only('true'), ['--only-changed']);
  assert.deepEqual(only('false'), ['--no-only-changed']);
  assert.deepEqual(only(''), []);
  assert.deepEqual(only(undefined), []);
});

test('buildArgs: only-changed, max-rows and base-format pass through', () => {
  const none = buildArgs(base);
  for (const f of ['--only-changed', '--max-rows', '--base-format']) assert.ok(!none.includes(f));
  const args = buildArgs({ ...base, onlyChanged: 'true', maxRows: '3', baseFormat: 'benchdiff' });
  assert.ok(args.includes('--only-changed'));
  assert.equal(flagValue(args, '--max-rows'), '3');
  assert.equal(flagValue(args, '--base-format'), 'benchdiff');
});

test('parseFailOnRegression normalizes case/space and rejects junk', () => {
  assert.equal(parseFailOnRegression(' TRUE '), 'true');
  assert.equal(parseFailOnRegression('False'), 'false');
  assert.equal(parseFailOnRegression(''), '');
  assert.equal(parseFailOnRegression(undefined), '');
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
  const s = setup('', path.join(os.tmpdir(), 'bd-does-not-exist.txt'));
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
  const args = buildArgs({ head: ['h.txt'], base: ['b.txt'], format: '', threshold: '', minAbs: '', sort: '', title: '', tag: '', failOnRegression: '' });
  assert.deepEqual(args, ['--head', 'h.txt', '--base', 'b.txt', ...TAIL]);
});

test('buildArgs: explicitly set defaults are still emitted (inputs override the config file)', () => {
  const args = buildArgs({ head: ['h.txt'], base: ['b.txt'], format: 'auto', threshold: '5', minAbs: '0', sort: 'delta', title: 'Benchmark comparison', tag: 'default', failOnRegression: '' });
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
    const s = setup('');
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
    onlyChanged: 'true', sort: 'name', maxRows: '9', title: 'My title', tag: 'perf', failOnRegression: '10',
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
  assert.deepEqual(buildArgs({ ...base, onlyChanged: 'true' }).filter((a) => a === '--only-changed'), ['--only-changed']);
  assert.deepEqual(buildArgs({ ...base, onlyChanged: 'false' }).filter((a) => a === '--only-changed'), []);
  assert.deepEqual(buildArgs({ ...base, onlyChanged: 'false' }).filter((a) => a === '--no-only-changed'), ['--no-only-changed']);
  assert.deepEqual(tail({ failOnRegression: 'false' }), ['--no-fail-on-regression']);
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

test('clip_should_move_cut_before_image_or_picture_markup_when_cut_inside', () => {
  const marker = '<!-- benchdiff:test -->';
  const url = 'https://raw.githubusercontent.com/o/r/benchdiff-assets/a.svg';
  const notice = '_Report truncated';
  const check = (out, bad) => {
    assert.ok(out.length < 65536);
    assert.ok(!out.includes(bad), `kept ${bad}`);
    assert.ok(out.includes(notice));
    assert.ok(out.trimEnd().endsWith(marker));
  };

  // Image on one long line with no newline near the limit: raw cut would land inside it.
  const img = `![chart](${url})`;
  const imgBody = `${marker}\n${'x'.repeat(59990 - marker.length - 1)}${img}\n${'row\n'.repeat(3000)}`;
  assert.ok(imgBody.indexOf(img) < 60000 && imgBody.indexOf(img) + img.length > 60000);
  check(clip(imgBody, marker), '![');

  // Multi-line <picture> block straddling the limit.
  const pic = `<picture>\n<source media="(prefers-color-scheme: dark)" srcset="${url}">\n<img alt="c" src="${url}">\n</picture>`;
  const pad = 'x'.repeat(59990 - marker.length - 1) + '\n';
  const picBody = `${marker}\n${pad}${pic}\n${'row\n'.repeat(3000)}`;
  check(clip(picBody, marker), '<picture>');

  // Closed details around a cut-before-picture stays balanced.
  const out = clip(`${marker}\n<details>\n${pad.slice(0, -1)}\n${pic}\n${'row\n'.repeat(3000)}</details>\n`, marker);
  assert.equal((out.match(/<details>/g) || []).length, (out.match(/<\/details>/g) || []).length);
  assert.ok(!out.includes('<picture>'));
});

test('buildArgs: new inputs emit their flag only when non-empty', () => {
  const none = buildArgs(base);
  for (const f of ['--compare-by', '--group-by', '--dimensions', '--dimensions-strict', '--reference', '--correction', '--cell-format',
    '--charts', '--chart-rows', '--chart-width', '--chart-theme', '--svg-dir', '--publish-images']) assert.ok(!none.includes(f), f);
  assert.ok(!none.some((a) => a.startsWith('--svg-embed')));
  const args = buildArgs({
    ...base, compareBy: 'impl', groupBy: 'size', reference: 'quick', correction: 'none', cellFormat: 'ratio',
    charts: 'delta,ranking', chartRows: '10', chartWidth: '800', chartTheme: 'both', svgDir: '/work/svg', publishImages: 'false',
  });
  assert.equal(flagValue(args, '--compare-by'), 'impl');
  assert.equal(flagValue(args, '--group-by'), 'size');
  assert.equal(flagValue(args, '--reference'), 'quick');
  assert.equal(flagValue(args, '--correction'), 'none');
  assert.equal(flagValue(args, '--cell-format'), 'ratio');
  assert.equal(flagValue(args, '--charts'), 'delta,ranking');
  assert.equal(flagValue(args, '--chart-rows'), '10');
  assert.equal(flagValue(args, '--chart-width'), '800');
  assert.equal(flagValue(args, '--chart-theme'), 'both');
  assert.equal(flagValue(args, '--svg-dir'), '/work/svg');
  assert.ok(args.includes('--publish-images=false'));
});

test('buildArgs: svg-embed is auto when unset and the user value when set', () => {
  assert.ok(buildArgs({ ...base, svgEmbed: '' }).includes('--svg-embed=auto'));
  assert.ok(buildArgs({ ...base, svgEmbed: 'false' }).includes('--svg-embed=false'));
  assert.ok(buildArgs({ ...base, svgEmbed: 'true' }).includes('--svg-embed=true'));
});

test('buildArgs: dimensions repeats the flag once per non-blank line, in order', () => {
  const args = buildArgs({ ...base, dimensions: 'impl=/impl=(?P<impl>\\w+)\n\n  size=/size=(?P<size>\\d+)  \n' });
  const rules = args.flatMap((a, i) => (a === '--dimensions' ? [args[i + 1]] : []));
  assert.deepEqual(rules, ['impl=/impl=(?P<impl>\\w+)', 'size=/size=(?P<size>\\d+)']);
  assert.ok(!buildArgs({ ...base, dimensions: ' \n ' }).includes('--dimensions'));
});

test('buildArgs: dimensions-strict is a bare flag only when true (there is no negative flag)', () => {
  assert.ok(buildArgs({ ...base, dimensionsStrict: 'true' }).includes('--dimensions-strict'));
  assert.ok(!buildArgs({ ...base, dimensionsStrict: 'false' }).includes('--dimensions-strict'));
  assert.ok(!buildArgs({ ...base, dimensionsStrict: '' }).includes('--dimensions-strict'));
});

test('buildArgs: sort accepts rank and passes it through', () => {
  assert.equal(flagValue(buildArgs({ ...base, sort: 'rank' }), '--sort'), 'rank');
});

// ---- Task 4.2.1b: chart pipeline in main() with an injected wasm runner and fetch ----

const MARKER = '<!-- benchdiff:default -->';
const SVG = '<svg xmlns="http://www.w3.org/2000/svg"><title>t</title></svg>';
const CHART_MD = `${MARKER}\n## T\n\n![Delta chart](benchdiff-svg:delta.svg)\n\n<details>\n<summary>Chart data (text)</summary>\n\nrows\n\n</details>\n\n| a | b |\n`;

/** Fake wasm: records argv, asserts svg/ exists, writes the report files and the given SVGs. */
function fakeWasm({ markdown = CHART_MD, svgs = ['delta'], stderr = 'charts: produced=1 skipped=0\n', code = 0 } = {}) {
  const calls = [];
  const run = async (args, workDir) => {
    calls.push({ args, svgExisted: fs.existsSync(path.join(workDir, 'svg')) });
    fs.writeFileSync(path.join(workDir, 'report.md'), markdown);
    fs.writeFileSync(path.join(workDir, 'report.json'), JSON.stringify({ counts: { regressed: 0, improved: 0, unchanged: 1, new: 0, removed: 0 }, failed: false }));
    fs.writeFileSync(path.join(workDir, 'head.json'), '{}');
    for (const n of svgs) fs.writeFileSync(path.join(workDir, 'svg', `${n}.svg`), SVG);
    return { code, stderr, stdout: '' };
  };
  return { run, calls };
}

/** Recording fetch emulating just enough of the Git Data API for publishImages and the comment upsert. */
function fakeGithub({ failStatus } = {}) {
  const requests = [];
  const fetchImpl = async (url, init = {}) => {
    const method = init.method || 'GET';
    requests.push({ method, url, body: init.body });
    const json = (status, body) => ({ status, ok: status < 300, headers: { get: () => null }, json: async () => body });
    if (url.includes('/issues/')) return method === 'GET' ? json(200, []) : json(201, { id: 9 });
    if (failStatus) return json(failStatus, { message: 'Resource not accessible by integration' });
    if (url.endsWith('/git/blobs')) return json(201, { sha: 'b'.repeat(40) });
    if (url.includes('/git/ref/heads/')) return json(404, {});
    if (url.endsWith('/git/trees')) return json(201, { sha: 't'.repeat(40) });
    if (url.endsWith('/git/commits')) return json(201, { sha: 'c'.repeat(40) });
    return json(201, {});
  };
  return { fetchImpl, requests, publishCalls: () => requests.filter((r) => r.url.includes('/git/')) };
}

function chartSetup(extra = {}, { event = 'same-repo' } = {}) {
  const s = setup('');
  const eventPath = path.join(path.dirname(s.output), 'event.json');
  const repo = { full_name: 'o/r' };
  fs.writeFileSync(eventPath, JSON.stringify({
    repository: { private: false },
    pull_request: { number: 7, head: { repo: event === 'fork' ? { full_name: 'x/r' } : repo }, base: { repo } },
  }));
  Object.assign(s.env, {
    INPUT_COMMENT: 'true', GITHUB_TOKEN: 'ghs_secret', GITHUB_REPOSITORY: 'o/r', GITHUB_EVENT_NAME: 'pull_request',
    GITHUB_EVENT_PATH: eventPath, GITHUB_SERVER_URL: 'https://github.com', GITHUB_WORKSPACE: path.dirname(s.output),
    INPUT_CHARTS: 'delta', ...extra,
  });
  return s;
}

async function runCaptured(s, fake, gh) {
  const logs = [];
  const orig = process.stdout.write;
  process.stdout.write = (c) => { logs.push(String(c)); return true; };
  try {
    await withEnv(s.env, () => main({ runWasm: fake.run, fetchImpl: gh.fetchImpl }));
  } finally {
    process.stdout.write = orig;
  }
  return logs.join('');
}

const outputOf = (s, key) => {
  const text = fs.readFileSync(s.output, 'utf8');
  const m = new RegExp(`^${key}=(.*)$|^${key}<<(\\S+)\\n([\\s\\S]*?)\\n\\2$`, 'm').exec(text);
  return m ? (m[1] ?? m[3]) : undefined;
};
const commentBody = (gh) => JSON.parse(gh.requests.find((r) => r.method === 'POST' && r.url.includes('/issues/')).body).body;

test('main_should_embed_hosted_url_in_summary_and_comment_and_list_svg_paths_when_charts_delta_alone', async () => {
  const s = chartSetup();
  const fake = fakeWasm();
  const gh = fakeGithub();
  const log = await runCaptured(s, fake, gh);
  const args = fake.calls[0].args;
  assert.ok(fake.calls[0].svgExisted, 'svg/ must exist before the wasm runs');
  assert.equal(flagValue(args, '--svg-dir'), '/work/svg');
  assert.equal(flagValue(args, '--charts'), 'delta');
  assert.ok(args.includes('--svg-embed=auto'));
  const url = `https://raw.githubusercontent.com/o/r/${'c'.repeat(40)}/`;
  for (const body of [fs.readFileSync(s.summary, 'utf8'), commentBody(gh)]) {
    assert.ok(body.startsWith(MARKER));
    assert.ok(body.includes(url) && body.trimEnd().endsWith('| a | b |') && !body.includes('benchdiff-svg:'));
  }
  assert.match(outputOf(s, 'svg-paths'), /delta\.svg$/);
  assert.ok(fs.existsSync(outputOf(s, 'svg-paths')));
  assert.match(log, /^charts: requested=delta produced=1 skipped=0$/m);
  assert.match(log, /^images: publishing to branch benchdiff-assets \(needs contents: write; commits are kept; see README "Hosting images"\)$/m);
  assert.match(log, new RegExp(`^images: published 1 file to benchdiff-assets@${'c'.repeat(40)}$`, 'm'));
  assert.ok(!log.includes('ghs_secret'));
});

test('main_should_log_skipped_reason_and_copy_svgs_to_svg_dir_when_kinds_are_skipped', async () => {
  const s = chartSetup({ INPUT_CHARTS: 'delta,ranking', 'INPUT_SVG-DIR': 'out/svgs' });
  const fake = fakeWasm({ stderr: 'charts: produced=1 skipped=1\nnotice: ranking chart skipped: ranking needs an N-way run\n' });
  const log = await runCaptured(s, fake, fakeGithub());
  assert.match(log, /^charts: requested=delta,ranking produced=1 skipped=1 \(ranking chart skipped: ranking needs an N-way run\)$/m);
  const dir = path.join(s.env.GITHUB_WORKSPACE, 'out/svgs');
  assert.equal(outputOf(s, 'svg-dir'), dir);
  assert.equal(fs.readFileSync(path.join(dir, 'delta.svg'), 'utf8'), SVG);
});

test('main_should_make_zero_publish_calls_and_leave_markdown_alone_when_svg_embed_false', async () => {
  const s = chartSetup({ 'INPUT_SVG-EMBED': 'false' });
  const plain = `${MARKER}\n## T\n\n| a | b |\n`;
  const gh = fakeGithub();
  await runCaptured(s, fakeWasm({ markdown: plain }), gh);
  assert.equal(gh.publishCalls().length, 0);
  assert.equal(commentBody(gh), plain);
  assert.match(outputOf(s, 'svg-paths'), /delta\.svg$/);
});

test('main_should_make_zero_publish_calls_when_publish_images_false_and_keep_core_markdown', async () => {
  const s = chartSetup({ 'INPUT_PUBLISH-IMAGES': 'false' });
  const md = `${MARKER}\n## T\n\ncharts not embedded: publish-images is false; SVGs written to /work/svg\n\n<details open>\n<summary>Chart data (text)</summary>\n\nrows\n\n</details>\n`;
  const gh = fakeGithub();
  await runCaptured(s, fakeWasm({ markdown: md }), gh);
  assert.equal(gh.publishCalls().length, 0);
  assert.equal(commentBody(gh), md);
});

test('main_should_not_error_and_embed_nothing_when_svg_embed_true_and_charts_empty', async () => {
  const s = chartSetup({ INPUT_CHARTS: '', 'INPUT_SVG-EMBED': 'true' });
  const plain = `${MARKER}\n## T\n`;
  const fake = fakeWasm({ markdown: plain, svgs: [] });
  const gh = fakeGithub();
  await runCaptured(s, fake, gh);
  assert.ok(!fake.calls[0].args.includes('--svg-dir'));
  assert.equal(gh.publishCalls().length, 0);
});

test('main_should_post_permission_line_with_snippet_pointer_open_text_block_and_pass_when_charts_delta_alone_meets_read_only_token', async () => {
  const s = chartSetup();
  const gh = fakeGithub({ failStatus: 403 });
  const log = await runCaptured(s, fakeWasm(), gh);
  const body = commentBody(gh);
  assert.ok(body.startsWith(MARKER));
  assert.ok(body.includes('charts could not be hosted: token lacks contents: write (grant it to host images); add the permissions block from README "Hosting images"'));
  assert.ok(body.includes('<details open>\n<summary>Chart data (text)'));
  assert.ok(!body.includes('benchdiff-svg:') && !body.includes('raw.githubusercontent.com'));
  assert.match(outputOf(s, 'svg-paths'), /delta\.svg$/);
  assert.equal(log.split('contents: write        # host chart images').length - 1, 1, 'snippet printed once');
  assert.match(log, /::warning::.*charts could not be hosted/);
});

test('main_should_fall_back_to_markdown_and_log_fork_line_when_pr_is_from_a_fork', async () => {
  const s = chartSetup({}, { event: 'fork' });
  const gh = fakeGithub();
  const log = await runCaptured(s, fakeWasm(), gh);
  assert.match(log, /^images: not hosted \(fork PR\); falling back to Markdown$/m);
  assert.equal(gh.publishCalls().length, 0);
  assert.ok(commentBody(gh).includes('read-only token on a fork PR'));
});

test('main_should_restore_original_golden_body_when_a_later_run_unsets_charts', async () => {
  const plain = `${MARKER}\n## T\n\n| a | b |\n`;
  const first = chartSetup();
  const gh1 = fakeGithub();
  await runCaptured(first, fakeWasm(), gh1);
  const second = chartSetup({ INPUT_CHARTS: '' });
  const gh2 = fakeGithub();
  await runCaptured(second, fakeWasm({ markdown: plain, svgs: [] }), gh2);
  const body = commentBody(gh2);
  assert.equal(body, plain);
  assert.ok(!body.includes('raw.githubusercontent.com') && !body.includes('benchdiff-svg:'));
});

test('main_should_pass_the_job_with_a_warning_when_the_binary_cannot_write_the_svg_dir', async () => {
  const s = chartSetup();
  const md = `${MARKER}\n## T\n\n| a | b |\n`;
  const fake = fakeWasm({ markdown: md, svgs: [], code: 2, stderr: 'cannot write /work/svg: permission denied; report printed without charts\n' });
  const log = await runCaptured(s, fake, fakeGithub());
  assert.match(log, /::warning::cannot write \/work\/svg: permission denied; report printed without charts; the table is complete/);
});

test('main_should_still_fail_on_a_wasm_config_error', async () => {
  const s = chartSetup();
  const fake = { calls: [], run: async () => ({ code: 2, stderr: 'config: threshold: "x" is not a number', stdout: '' }) };
  await assert.rejects(runCaptured(s, fake, fakeGithub()), /threshold/);
});

test('main_should_keep_no_chart_outputs_and_inputs_untouched_on_the_default_path', async () => {
  const s = setup('50');
  await withEnv(s.env, () => main());
  const out = fs.readFileSync(s.output, 'utf8');
  assert.ok(!/svg-/.test(out));
  const golden = fs.readFileSync(path.join(root, 'testdata/golden/expected/action-default.md'), 'utf8');
  assert.equal(fs.readFileSync(s.summary, 'utf8'), `${golden}\n`);
});

test('action.yml: count outputs say N-way runs compare cells vs the reference', () => {
  const yml = fs.readFileSync(path.join(root, 'action.yml'), 'utf8');
  const outputs = yml.slice(yml.indexOf('\noutputs:'));
  assert.ok((outputs.match(/N-way runs: cells vs the reference/g) || []).length >= 5);
});
