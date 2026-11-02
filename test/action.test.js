'use strict';
// Action-level tests: argument building, input validation and an end-to-end main() run.
// Needs the committed dist/benchdiff.wasm only (no native binary).

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { buildArgs, stageConfig, parseFailOnRegression, validateTag, clip, main, expand, setOutputs } = require('../index.js');

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

// Fixtures are copied into the temp dir, which doubles as GITHUB_WORKSPACE (paths must stay inside it).
function setup(limit, head) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-main-'));
  const fixture = (name) => { const to = path.join(dir, name); fs.copyFileSync(path.join(root, 'testdata/golden', name), to); return to; };
  if (head === undefined) head = fixture('head.go.txt');
  const output = path.join(dir, 'output');
  const summary = path.join(dir, 'summary');
  const env = {
    INPUT_HEAD: head,
    INPUT_BASE: fixture('base.go.txt'),
    GITHUB_WORKSPACE: dir,
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

test('stageConfig copies the config byte for byte under its own sanitized base name', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-cfg-'));
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-ws-'));
  const text = '{"version":1,\n "threshold":3}';
  fs.writeFileSync(path.join(ws, 'benchdiff.config.json'), text);
  fs.writeFileSync(path.join(ws, 'my config (1).json'), text);
  await withEnv({ GITHUB_WORKSPACE: ws }, () => {
    assert.equal(stageConfig('benchdiff.config.json', dir, ws), '/work/config/benchdiff.config.json');
    assert.equal(fs.readFileSync(path.join(dir, 'config', 'benchdiff.config.json'), 'utf8'), text);
    assert.equal(stageConfig('my config (1).json', dir, ws), '/work/config/my_config__1_.json');
    assert.throws(() => stageConfig('nope.json', dir, ws), /config file not found: nope\.json/);
  });
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
function fakeWasm({ markdown = CHART_MD, svgs = ['delta'], stderr = 'charts: produced=1 skipped=0\n', code = 0, svgContent = SVG } = {}) {
  const calls = [];
  const run = async (args, workDir) => {
    calls.push({ args, svgExisted: fs.existsSync(path.join(workDir, 'svg')) });
    fs.writeFileSync(path.join(workDir, 'report.md'), markdown);
    fs.writeFileSync(path.join(workDir, 'report.json'), JSON.stringify({ counts: { regressed: 0, improved: 0, unchanged: 1, new: 0, removed: 0 }, failed: false }));
    fs.writeFileSync(path.join(workDir, 'head.json'), '{}');
    for (const n of svgs) fs.writeFileSync(path.join(workDir, 'svg', `${n}.svg`), svgContent);
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

// ---- Task 4.2.3a: `variants` input (parsing, resolution, staging, buildArgs, guard) ----

const { parseVariants, resolveOne, resolveVariants, stageVariants } = require('../index.js');

// Variant tests share one temp GITHUB_WORKSPACE holding a copy of testdata/nway (paths must stay inside it).
const VWS = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'bd-vws-')));
function variantDir(files) {
  const dir = fs.mkdtempSync(path.join(VWS, 'bd-variants-'));
  for (const [name, text] of Object.entries(files)) {
    fs.mkdirSync(path.dirname(path.join(dir, name)), { recursive: true });
    fs.writeFileSync(path.join(dir, name), text);
  }
  return dir;
}

test('parseVariants: splits at the first =, keeps = in paths, skips blank lines, commas are not separators', () => {
  assert.deepEqual(parseVariants('quick=out/quick.txt\n\nmerge=results/size=100/m.txt\ng={a,b}.txt'), [
    { label: 'quick', path: 'out/quick.txt' },
    { label: 'merge', path: 'results/size=100/m.txt' },
    { label: 'g', path: '{a,b}.txt' },
  ]);
});

test('parseVariants: a line without a valid label names its 1-based line, text and the grammar', () => {
  assert.throws(() => parseVariants('a=x\nb=y\nresults/size=100/m.txt'),
    { message: "variants: line 3 'results/size=100/m.txt' has no valid label; expected label=path (label: A-Za-z0-9._-, max 40)" });
  for (const bad of ['nolabel', '=x', 'a b=x', `${'l'.repeat(41)}=x`]) assert.throws(() => parseVariants(bad), /has no valid label/, bad);
  assert.doesNotThrow(() => parseVariants(`${'l'.repeat(40)}=x`));
});

test('parseVariants: a line with a label but an empty path is rejected', () => {
  assert.throws(() => parseVariants('quick='), /has no valid label|no path/);
});

test('parseVariants: duplicate label is a static error', () => {
  assert.throws(() => parseVariants('quick=a\nheap=b\nquick=c'), { message: 'variants: label "quick" given twice' });
});

test('resolveOne: a direct file wins, a glob is sorted, braces are one pattern and commas do not split', () => {
  const dir = variantDir({ 'out/b.txt': 'b', 'out/a.txt': 'a', 'out/c.json': 'c', 'x,y.txt': 'xy' });
  assert.deepEqual(resolveOne('out/*.txt', dir), [path.join(dir, 'out/a.txt'), path.join(dir, 'out/b.txt')]);
  assert.deepEqual(resolveOne('out/{a,b}.txt', dir), [path.join(dir, 'out/a.txt'), path.join(dir, 'out/b.txt')]);
  assert.deepEqual(resolveOne('out/c.json', dir), [path.join(dir, 'out/c.json')]);
  assert.deepEqual(resolveOne('x,y.txt', dir), [path.join(dir, 'x,y.txt')]);
  assert.deepEqual(resolveOne('out/none*.txt', dir), []);
});

test('resolveVariants: one label is exactly one file; a multi-match names the matches; zero matches passes through', () => {
  const dir = variantDir({ 'out/quick.txt': 'q', 'out/h1.txt': '1', 'out/h2.txt': '2', 'out/h3.txt': '3' });
  const ok = resolveVariants([{ label: 'quick', path: 'out/quick.txt' }, { label: 'gone', path: 'out/missing*.txt' }], dir);
  assert.deepEqual(ok, [{ label: 'quick', path: 'out/quick.txt', file: path.join(dir, 'out/quick.txt') }, { label: 'gone', path: 'out/missing*.txt', file: undefined }]);
  assert.throws(() => resolveVariants([{ label: 'heap', path: 'out/h*.txt' }], dir),
    (e) => e.message.startsWith("variants: label 'heap' matched 3 files; a label is one file") && /h1\.txt/.test(e.message) && /h3\.txt/.test(e.message));
  assert.throws(() => resolveVariants([{ label: 'merge', path: 'out/h{1,2}.txt' }], dir), /label 'merge' matched 2 files/);
  assert.equal(resolveVariants([{ label: 'merge', path: 'out/h{1,9}.txt' }], dir)[0].file, path.join(dir, 'out/h1.txt'));
});

test('stageVariants: copies bytes under variant-<index>-<sanitized basename> (no =) and emits label=/work paths in input order', () => {
  const src = variantDir({ 'a/quick.txt': 'quick-bytes', 'b/we ird=name.txt': 'heap-bytes' });
  const work = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-work-'));
  const out = stageVariants([
    { label: 'quick', file: path.join(src, 'a/quick.txt') },
    { label: 'heap', file: path.join(src, 'b/we ird=name.txt') },
    { label: 'gone', file: undefined },
  ], work);
  assert.deepEqual(out, ['quick=/work/variant-0-quick.txt', 'heap=/work/variant-1-we_ird_name.txt']);
  assert.equal(fs.readFileSync(path.join(work, 'variant-0-quick.txt'), 'utf8'), 'quick-bytes');
  assert.equal(fs.readFileSync(path.join(work, 'variant-1-we_ird_name.txt'), 'utf8'), 'heap-bytes');
});

test('buildArgs: variants emit one repeated --variant per line and no --head-json; two-way keeps --head-json', () => {
  const args = buildArgs({ ...base, head: [], base: [], variants: ['quick=/work/variant-0-q.txt', 'heap=/work/variant-1-h.txt'] });
  assert.deepEqual(args.filter((a, i) => args[i - 1] === '--variant'), ['quick=/work/variant-0-q.txt', 'heap=/work/variant-1-h.txt']);
  assert.equal(args.filter((a) => a === '--variant').length, 2);
  assert.ok(!args.includes('--head-json'));
  assert.ok(!args.includes('--head') && !args.includes('--base'));
  assert.ok(buildArgs(base).includes('--head-json'));
  assert.ok(!buildArgs(base).includes('--variant'));
});

function variantSetup(variants, extra = {}) {
  const s = setup('');
  delete s.env.INPUT_HEAD;
  delete s.env.INPUT_BASE;
  s.env.GITHUB_WORKSPACE = VWS;
  s.env.INPUT_VARIANTS = variants;
  Object.assign(s.env, extra);
  return s;
}

test('main: variants together with head or base is a static error and runs nothing', async () => {
  const src = variantDir({ 'q.txt': 'q' });
  for (const extra of [{ INPUT_HEAD: 'x.txt' }, { INPUT_BASE: 'y.txt' }]) {
    const s = variantSetup(`quick=${path.join(src, 'q.txt')}`, extra);
    const fake = fakeWasm();
    await withEnv(s.env, async () => {
      await assert.rejects(main({ runWasm: fake.run }), { message: 'variants cannot be combined with head/base' });
    });
    assert.equal(fake.calls.length, 0);
  }
});

test('main: base-format together with variants is a static error', async () => {
  const src = variantDir({ 'q.txt': 'q' });
  const s = variantSetup(`quick=${path.join(src, 'q.txt')}`, { 'INPUT_BASE-FORMAT': 'go' });
  const fake = fakeWasm();
  await withEnv(s.env, async () => {
    await assert.rejects(main({ runWasm: fake.run }), { message: 'base-format cannot be combined with variants' });
  });
  assert.equal(fake.calls.length, 0);
});

test('main: variants alone skips the head guard, stages files, passes --variant and leaves head-json-path empty', async () => {
  const src = variantDir({ 'out/quick.txt': 'quick-bytes', 'out/heap-1.txt': 'heap-bytes' });
  const s = variantSetup(`quick=${path.join(src, 'out/quick.txt')}\nheap=${path.join(src, 'out/*heap*.txt')}`, { INPUT_REFERENCE: 'quick' });
  const staged = [];
  const fake = fakeWasm({ markdown: `${MARKER}\n## T\n`, svgs: [], stderr: '' });
  const run = async (args, workDir) => {
    for (const a of args) if (a.includes('=/work/variant-')) staged.push(fs.readFileSync(path.join(workDir, a.split('=/work/')[1]), 'utf8'));
    return fake.run(args, workDir);
  };
  await withEnv(s.env, () => main({ runWasm: run }));
  const args = fake.calls[0].args;
  assert.deepEqual(args.filter((a, i) => args[i - 1] === '--variant'), ['quick=/work/variant-0-quick.txt', 'heap=/work/variant-1-heap-1.txt']);
  assert.deepEqual(staged, ['quick-bytes', 'heap-bytes']);
  assert.ok(!args.includes('--head') && !args.includes('--base') && !args.includes('--head-json'));
  assert.equal(flagValue(args, '--reference'), 'quick');
  assert.equal(outputOf(s, 'head-json-path'), '');
  assert.match(outputOf(s, 'json-path'), /report\.json$/);
});

test('main: a multi-file glob for one label fails the step before any run', async () => {
  const src = variantDir({ 'h1.txt': '1', 'h2.txt': '2' });
  const s = variantSetup(`heap=${path.join(src, 'h*.txt')}`);
  const fake = fakeWasm();
  await withEnv(s.env, async () => {
    await assert.rejects(main({ runWasm: fake.run }), /label 'heap' matched 2 files; a label is one file/);
  });
  assert.equal(fake.calls.length, 0);
});

test('main: head empty and variants empty keeps the existing no-head error', async () => {
  const s = variantSetup('');
  await withEnv(s.env, async () => {
    await assert.rejects(main({ runWasm: fakeWasm().run }), /no head files match/);
  });
});

test('action.yml: declares an optional variants input and head is no longer required', () => {
  const yml = fs.readFileSync(path.join(root, 'action.yml'), 'utf8');
  const block = (name) => new RegExp(`^  ${name}:\\n((?:    .*\\n)+)`, 'm').exec(yml)?.[1] ?? '';
  assert.match(block('variants'), /required: false/);
  assert.match(block('variants'), /label=path/);
  assert.match(block('head'), /required: false/);
});

// ---- Task 4.2.3b: `variants` data errors and matrix outputs ----

const { renderNoticeReport } = require('../index.js');
const NOTICE_GOLDEN = path.join(root, 'testdata/nway/expected/notice-only.md');
const REF_LOST = "reference 'quick' produced no data; no comparison made";

test('renderNoticeReport: equals the committed notice-only golden the Rust snapshot also asserts', () => {
  const got = renderNoticeReport({ title: 'Benchmark comparison', tag: 'default', notices: [REF_LOST] });
  assert.equal(got, fs.readFileSync(NOTICE_GOLDEN, 'utf8'));
});

test('renderNoticeReport: escapes like the Rust renderer and falls back to the benchdiff title', () => {
  const got = renderNoticeReport({ title: '  ', tag: 'a b', notices: ['x_y <z> `c`'] });
  assert.equal(got, '<!-- benchdiff:a_b -->\n### benchdiff\n\n\u26a0\ufe0f x\\_y &lt;z&gt; \\`c\\`\n\n');
});

async function runNoticeOnly(variants, extra = {}) {
  const s = variantSetup(variants, { INPUT_COMMENT: 'true', GITHUB_TOKEN: 't', GITHUB_REPOSITORY: 'o/r', ...extra });
  const eventPath = path.join(path.dirname(s.output), 'event.json');
  fs.writeFileSync(eventPath, JSON.stringify({ pull_request: { number: 7 } }));
  s.env.GITHUB_EVENT_PATH = eventPath;
  const fake = fakeWasm();
  const gh = fakeGithub();
  await withEnv(s.env, () => main({ runWasm: fake.run, fetchImpl: gh.fetchImpl }));
  return { s, fake, gh };
}

test('main: a missing reference file posts the notice-only report without running the binary and passes', async () => {
  const src = variantDir({ 'heap.txt': 'h' });
  const { s, fake, gh } = await runNoticeOnly(`quick=${path.join(src, 'gone.txt')}\nheap=${path.join(src, 'heap.txt')}`, { INPUT_REFERENCE: 'quick', 'INPUT_FAIL-ON-REGRESSION': '5' });
  const golden = fs.readFileSync(NOTICE_GOLDEN, 'utf8');
  assert.equal(fake.calls.length, 0);
  assert.equal(commentBody(gh), golden);
  assert.equal(fs.readFileSync(s.summary, 'utf8'), `${golden}\n`);
  assert.equal(fs.readFileSync(outputOf(s, 'report-path'), 'utf8'), golden);
  for (const k of ['regressed', 'improved', 'unchanged', 'new', 'removed', 'incomparable', 'invalid']) assert.equal(outputOf(s, k), '0', k);
  assert.equal(outputOf(s, 'failed'), 'false');
});

test('main: the first variant is the reference when none is given', async () => {
  const src = variantDir({ 'heap.txt': 'h' });
  const { fake, gh } = await runNoticeOnly(`quick=${path.join(src, 'gone.txt')}\nheap=${path.join(src, 'heap.txt')}`);
  assert.equal(fake.calls.length, 0);
  assert.equal(commentBody(gh), fs.readFileSync(NOTICE_GOLDEN, 'utf8'));
});

test('main: fewer than two variants left names the dropped variant and the reason, without a binary run', async () => {
  const src = variantDir({ 'quick.txt': 'q' });
  const gone = path.join(src, 'gone.txt');
  const { fake, gh } = await runNoticeOnly(`quick=${path.join(src, 'quick.txt')}\nheap=${gone}`, { INPUT_REFERENCE: 'quick' });
  assert.equal(fake.calls.length, 0);
  assert.ok(!/[\\`*_[\]|@]/.test(gone), 'tmp path needs no escaping');
  assert.equal(commentBody(gh), `<!-- benchdiff:default -->\n### Benchmark comparison\n\n\u26a0\ufe0f variant 'heap' produced no data (${gone}); comparing quick\n\n\u26a0\ufe0f only quick has data; a matrix needs at least 2 variants; no comparison made\n\n`);
});

test('main: a missing non-reference variant stays in the run as an empty placeholder so the binary reports its own notice', async () => {
  const src = variantDir({ 'quick.txt': 'q', 'merge.txt': 'm' });
  const s = variantSetup(`quick=${path.join(src, 'quick.txt')}\nheap=${path.join(src, 'gone.txt')}\nmerge=${path.join(src, 'merge.txt')}`, { INPUT_REFERENCE: 'quick' });
  const seen = {};
  const fake = fakeWasm({ markdown: `${MARKER}\n## T\n`, svgs: [], stderr: '' });
  const run = async (args, workDir) => {
    for (const a of args) if (a.includes('=/work/variant-')) seen[a.split('=')[0]] = fs.readFileSync(path.join(workDir, a.split('=/work/')[1]), 'utf8');
    return fake.run(args, workDir);
  };
  await withEnv(s.env, () => main({ runWasm: run }));
  assert.deepEqual(seen, { quick: 'q', heap: '', merge: 'm' });
});

test('main: dimensions-strict turns each missing-variant case into the exit-2 text', async () => {
  const src = variantDir({ 'heap.txt': 'h' });
  const heap = path.join(src, 'heap.txt');
  const gone = path.join(src, 'gone.txt');
  for (const [variants, message] of [
    [`quick=${gone}\nheap=${heap}`, REF_LOST],
    [`quick=${heap}\nheap=${gone}`, `variant 'heap' produced no data (${gone})`],
  ]) {
    const s = variantSetup(variants, { INPUT_REFERENCE: 'quick', 'INPUT_DIMENSIONS-STRICT': 'true' });
    const fake = fakeWasm();
    await withEnv(s.env, async () => {
      await assert.rejects(main({ runWasm: fake.run }), { message });
    });
    assert.equal(fake.calls.length, 0);
  }
});

test('main: matrix JSON counts and failed reach the outputs unchanged and the gate still trips', async () => {
  const src = variantDir({ 'quick.txt': 'q', 'heap.txt': 'h' });
  const s = variantSetup(`quick=${path.join(src, 'quick.txt')}\nheap=${path.join(src, 'heap.txt')}`, { INPUT_REFERENCE: 'quick', 'INPUT_FAIL-ON-REGRESSION': '5' });
  const run = async (args, workDir) => {
    fs.writeFileSync(path.join(workDir, 'report.md'), `${MARKER}\n## T\n`);
    fs.writeFileSync(path.join(workDir, 'report.json'), JSON.stringify({
      counts: { regressed: 2, improved: 3, unchanged: 4, new: 1, removed: 5, incomparable: 6, invalid: 7 }, failed: true,
    }));
    return { code: 1, stderr: '', stdout: '' };
  };
  await withEnv(s.env, async () => {
    await assert.rejects(main({ runWasm: run }), /2 benchmark regression/);
  });
  const want = { regressed: '2', improved: '3', unchanged: '4', new: '1', removed: '5', incomparable: '6', invalid: '7', failed: 'true' };
  for (const [k, v] of Object.entries(want)) assert.equal(outputOf(s, k), v, k);
  assert.equal(outputOf(s, 'head-json-path'), '');
});

// ---- Task 4.2.3c: end-to-end `variants` runs through the real dist/benchdiff.wasm (mocked fetch only) ----

fs.cpSync(path.join(root, 'testdata/nway'), path.join(VWS, 'nway'), { recursive: true });
const NWAY = path.join(VWS, 'nway');
const nwayVariants = () => ['quick', 'heap', 'merge'].map((n) => `${n}=${path.join(NWAY, `${n}.txt`)}`).join('\n');
const MATRIX_MD = fs.readFileSync(path.join(NWAY, 'expected/matrix.md'), 'utf8');
const MATRIX_JSON = JSON.parse(fs.readFileSync(path.join(NWAY, 'expected/matrix.json'), 'utf8'));

/** Real-wasm variants run: no injected runWasm, only fetch is faked. Errors are returned, not thrown. */
async function runReal(extra = {}, variants = nwayVariants()) {
  const s = variantSetup(variants, {
    INPUT_COMMENT: 'true', GITHUB_TOKEN: 'ghs_t', GITHUB_REPOSITORY: 'o/r', GITHUB_SERVER_URL: 'https://github.com',
    GITHUB_EVENT_NAME: 'pull_request', ...extra,
  });
  const eventPath = path.join(path.dirname(s.output), 'event.json');
  const repo = { full_name: 'o/r' };
  fs.writeFileSync(eventPath, JSON.stringify({ repository: { private: false }, pull_request: { number: 7, head: { repo }, base: { repo } } }));
  s.env.GITHUB_EVENT_PATH = eventPath;
  const gh = fakeGithub();
  const logs = [];
  const orig = process.stdout.write;
  process.stdout.write = (c) => { logs.push(String(c)); return true; };
  let error;
  try {
    await withEnv(s.env, () => main({ fetchImpl: gh.fetchImpl }));
  } catch (e) {
    error = e;
  } finally {
    process.stdout.write = orig;
  }
  return { s, gh, error, log: logs.join('') };
}

test('action_variants_should_post_matrix_comment_equal_to_expected_when_three_labelled_lines', async () => {
  // Documented differences vs expected/matrix.md: none (the golden is generated with the Action's defaults:
  // title "Benchmark comparison", tag "default"; see plan.md Task 2.2.3a). The sticky marker is the first line.
  const r = await runReal({ INPUT_REFERENCE: 'quick' });
  assert.equal(r.error, undefined);
  const body = commentBody(r.gh);
  assert.equal(body.split('\n', 1)[0], '<!-- benchdiff:default -->');
  assert.equal(body, MATRIX_MD);
  assert.equal(fs.readFileSync(r.s.summary, 'utf8'), `${MATRIX_MD}\n`);
  for (const k of ['regressed', 'improved', 'unchanged', 'new', 'removed', 'incomparable', 'invalid']) assert.equal(outputOf(r.s, k), String(MATRIX_JSON.counts[k]), k);
  assert.equal(outputOf(r.s, 'failed'), 'false');
  assert.equal(outputOf(r.s, 'head-json-path'), '');
  assert.deepEqual(JSON.parse(fs.readFileSync(outputOf(r.s, 'json-path'), 'utf8')).counts, MATRIX_JSON.counts);
});

test('action_variants_should_fail_with_gate_text_when_fail_on_regression_has_explicit_reference_and_a_cell_regressed', async () => {
  // merge on Parse (ns/op) is +30.5% vs quick and significant: the only regressed cell.
  const r = await runReal({ INPUT_REFERENCE: 'quick', 'INPUT_FAIL-ON-REGRESSION': '5' });
  assert.match(r.error?.message ?? '', /^1 benchmark regression\(s\) exceed the fail-on-regression limit$/);
  assert.equal(outputOf(r.s, 'failed'), 'true');
  assert.equal(outputOf(r.s, 'regressed'), '1');
  const body = commentBody(r.gh);
  assert.match(body, /gate:/);
  assert.match(body, /merge/);
});

test('action_variants_should_fail_with_exit_2_text_when_fail_on_regression_has_no_explicit_reference', async () => {
  const r = await runReal({ 'INPUT_FAIL-ON-REGRESSION': '5' });
  assert.equal(r.error?.message, "benchdiff: fail-on-regression in an N-way run needs an explicit 'reference' (the baseline to protect); set 'reference'");
  assert.equal(r.gh.requests.length, 0, 'nothing is posted on a static error');
});

test('action_variants_should_post_notice_only_report_and_pass_when_reference_file_is_missing_then_fail_when_strict', async () => {
  const gone = path.join(os.tmpdir(), 'bd-e2e-no-such-quick.txt');
  const variants = `quick=${gone}\nheap=${path.join(NWAY, 'heap.txt')}\nmerge=${path.join(NWAY, 'merge.txt')}`;
  const r = await runReal({ INPUT_REFERENCE: 'quick', 'INPUT_FAIL-ON-REGRESSION': '5' }, variants);
  assert.equal(r.error, undefined);
  assert.equal(commentBody(r.gh), fs.readFileSync(NOTICE_GOLDEN, 'utf8'));
  assert.equal(outputOf(r.s, 'failed'), 'false');
  const strict = await runReal({ INPUT_REFERENCE: 'quick', 'INPUT_DIMENSIONS-STRICT': 'true' }, variants);
  assert.equal(strict.error?.message, REF_LOST);
  assert.equal(strict.gh.requests.length, 0);
});

test('action_variants_should_drop_empty_variant_post_matrix_and_pass_when_not_strict', async () => {
  const dir = fs.mkdtempSync(path.join(VWS, 'bd-e2e-empty-'));
  fs.writeFileSync(path.join(dir, 'heap.txt'), '');
  const variants = `quick=${path.join(NWAY, 'quick.txt')}\nheap=${path.join(dir, 'heap.txt')}\nmerge=${path.join(NWAY, 'merge.txt')}`;
  const r = await runReal({ INPUT_REFERENCE: 'quick', 'INPUT_FAIL-ON-REGRESSION': '100' }, variants);
  assert.equal(r.error, undefined);
  const body = commentBody(r.gh);
  assert.match(body, /variant 'heap' produced no data/);
  assert.match(body, /\| Benchmark \| quick \(ref\) \| merge \|/);
  assert.ok(!/\| heap \|/.test(body), 'dropped variant has no column');
  assert.equal(outputOf(r.s, 'failed'), 'false');
});

test('action_variants_should_reject_head_or_base_combined_with_variants_before_any_run', async () => {
  for (const extra of [{ INPUT_HEAD: path.join(NWAY, 'quick.txt') }, { INPUT_BASE: path.join(NWAY, 'quick.txt') }]) {
    const r = await runReal(extra);
    assert.equal(r.error?.message, 'variants cannot be combined with head/base');
    assert.equal(r.gh.requests.length, 0);
  }
});

test('action_variants_should_expose_svgs_and_strip_placeholders_when_charts_ranking_distribution_and_publish_images_false', async () => {
  const svgDir = path.join(fs.mkdtempSync(path.join(VWS, 'bd-e2e-svg-')), 'out');
  const r = await runReal({ INPUT_REFERENCE: 'quick', INPUT_CHARTS: 'ranking,distribution', 'INPUT_PUBLISH-IMAGES': 'false', 'INPUT_SVG-DIR': svgDir });
  assert.equal(r.error, undefined, r.error?.message);
  assert.equal(r.gh.publishCalls().length, 0, 'nothing hosted');
  const body = commentBody(r.gh);
  assert.ok(!body.includes('benchdiff-svg:'), 'placeholders stripped');
  assert.ok(body.includes('| Benchmark | quick (ref) | heap | merge | Rank |'), 'table kept');
  assert.equal(outputOf(r.s, 'svg-dir'), svgDir);
  const paths = outputOf(r.s, 'svg-paths').split('\n').filter(Boolean);
  assert.ok(paths.length >= 1, 'svg-paths set');
  for (const p of paths) assert.ok(fs.existsSync(p), p);
  assert.ok(fs.readdirSync(svgDir).some((n) => n.endsWith('.svg')));
  assert.equal(outputOf(r.s, 'failed'), 'false');
  // Both requested kinds are produced through the wasm (distribution needs samples; main.rs sets keep_samples for it).
  assert.match(r.log, /charts: requested=ranking,distribution produced=2 skipped=0\n/);
  assert.deepEqual(fs.readdirSync(svgDir).filter((n) => n.endsWith('.svg')).sort(), ['distribution.svg', 'ranking.svg']);
});

test('action_variants_should_pass_the_job_when_chart_hosting_fails_with_a_permission_error', async () => {
  const s = variantSetup(nwayVariants(), {
    INPUT_REFERENCE: 'quick', INPUT_CHARTS: 'ranking', INPUT_COMMENT: 'true', GITHUB_TOKEN: 'ghs_t', GITHUB_REPOSITORY: 'o/r',
    GITHUB_SERVER_URL: 'https://github.com', GITHUB_EVENT_NAME: 'pull_request',
  });
  const eventPath = path.join(path.dirname(s.output), 'event.json');
  const repo = { full_name: 'o/r' };
  fs.writeFileSync(eventPath, JSON.stringify({ repository: { private: false }, pull_request: { number: 7, head: { repo }, base: { repo } } }));
  s.env.GITHUB_EVENT_PATH = eventPath;
  const gh = fakeGithub({ failStatus: 403 });
  await withEnv(s.env, () => main({ fetchImpl: gh.fetchImpl }));
  assert.ok(!commentBody(gh).includes('benchdiff-svg:'));
  assert.equal(outputOf(s, 'failed'), 'false');
});

// ---- config input through the real wasm (WASI preopen of the staged config) ----

/** One benchmark that is +6.0% slower in head (single sample each, so only the threshold decides). */
function configSetup(configJson, extra = {}) {
  const s = setup('', undefined);
  const dir = path.dirname(s.output);
  fs.writeFileSync(path.join(dir, 'base.txt'), 'BenchmarkX-8 \t 1000 \t 1000.0 ns/op\n');
  fs.writeFileSync(path.join(dir, 'head.txt'), 'BenchmarkX-8 \t 1000 \t 1060.0 ns/op\n');
  fs.writeFileSync(path.join(dir, 'bd.config.json'), typeof configJson === 'string' ? configJson : JSON.stringify({ version: 1, ...configJson }));
  Object.assign(s.env, { INPUT_HEAD: path.join(dir, 'head.txt'), INPUT_BASE: path.join(dir, 'base.txt'), INPUT_CONFIG: path.join(dir, 'bd.config.json'), ...extra });
  return s;
}
const regressedOf = async (s) => {
  await withEnv(s.env, () => main());
  return outputOf(s, 'regressed');
};

test('action_config_should_apply_threshold_from_file_when_input_unset', async () => {
  assert.equal(await regressedOf(configSetup({ threshold: 7 })), '0', 'config 7 hides a +6% change');
  assert.equal(await regressedOf(configSetup({ threshold: 3 })), '1', 'config 3 flags it');
  const noConfig = configSetup({ threshold: 7 }, { INPUT_CONFIG: '' });
  assert.equal(await regressedOf(noConfig), '1', 'default threshold 5 flags it, so the config really reached the binary');
});

test('action_config_should_let_threshold_input_override_the_file_value', async () => {
  assert.equal(await regressedOf(configSetup({ threshold: 3 }, { INPUT_THRESHOLD: '7' })), '0', 'input 7 beats config 3');
  assert.equal(await regressedOf(configSetup({ threshold: 7 }, { INPUT_THRESHOLD: '3' })), '1', 'input 3 beats config 7');
});

test('action_config_should_fail_the_step_naming_file_and_key_when_config_has_an_unknown_key', async () => {
  const s = configSetup({ thresold: 3 });
  await withEnv(s.env, async () => {
    await assert.rejects(main(), /bd\.config\.json: unknown key 'thresold'; did you mean 'threshold'\?/);
  });
});

// ---- release workflow (plan Story 5.1.4) ----

test('release_workflow_should_upload_svg_paths_and_never_set_publish_images_when_charts_input', () => {
  const wf = fs.readFileSync(path.join(root, '.github/workflows/release-benchmarks.yml'), 'utf8');
  assert.match(wf, /charts: \$\{\{ inputs\.charts \}\}/);
  assert.match(wf, /config: \$\{\{ inputs\.config \}\}/);
  assert.match(wf, /SVG_PATHS: \$\{\{ steps\.diff\.outputs\.svg-paths \}\}/);
  assert.match(wf, /gh release upload "\$TAG" "\$\{assets\[@\]\}" --clobber/);
  assert.match(wf, /assets\+=\("\$svg"\)/);
  assert.match(wf, /^ {10}svg-embed: 'false'$/m, 'embedding is always off, even when a config file enables charts');
  assert.ok(!/^permissions:/m.test(wf), 'no workflow-level permissions');
  assert.match(wf, /^ {4}permissions:\n {6}contents: write$/m, 'contents: write is scoped to the job');
  const code = wf.split('\n').filter((l) => !l.trim().startsWith('#')).join('\n');
  assert.ok(!/publish-images/.test(code), 'workflow never sets publish-images');
});

// ---- Phase 6 review fixes ----

test('clip_should_balance_details_and_keep_the_notice_when_a_stripped_open_block_is_cut', () => {
  const rows = Array.from({ length: 30000 }, (_, i) => `row ${i}`).join('\n');
  const body = `${MARKER}\n## T\n\n<details open>\n<summary>Chart data (text)</summary>\n\n${rows}\n\n</details>\n`;
  const out = clip(body, MARKER);
  assert.equal((out.match(/<details[\s>]/g) || []).length, (out.match(/<\/details>/g) || []).length);
  assert.ok(out.indexOf('Report truncated') > out.lastIndexOf('<details open>'));
  assert.ok(out.indexOf('Report truncated') > out.lastIndexOf('</details>') - 1);
  assert.ok(out.trimEnd().endsWith(MARKER));
});

test('expand_should_skip_a_dangling_symlink_and_use_the_valid_file', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bd-dangle-'));
  fs.writeFileSync(path.join(dir, 'a.txt'), 'x');
  fs.symlinkSync(path.join(dir, 'missing-target'), path.join(dir, 'b.txt'));
  assert.deepEqual(expand('*.txt', dir), [path.join(dir, 'a.txt')]);
  assert.deepEqual(expand('b.txt', dir), []);
  assert.deepEqual(resolveOne('*.txt', dir), [path.join(dir, 'a.txt')]);
});

test('main_should_report_no_files_match_when_the_only_glob_hit_is_a_dangling_symlink', async () => {
  const s = wsSetup();
  fs.symlinkSync(path.join(s.ws, 'nowhere'), path.join(s.ws, 'dangling.txt'));
  s.env.INPUT_HEAD = path.join(s.ws, 'dangling.txt');
  await withEnv(s.env, async () => {
    await assert.rejects(main({ runWasm: fakeWasm({ svgs: [] }).run }), /no head files match/);
  });
});

test('logLine_should_strip_carriage_returns_and_control_characters_from_chart_skip_reasons', async () => {
  const s = chartSetup();
  const fake = fakeWasm({ stderr: 'charts: produced=1 skipped=1\nnotice: delta chart skipped: bad\rname\u001b[31m\u0007 here\n' });
  const log = await runCaptured(s, fake, fakeGithub());
  assert.ok(!/[\r\u001b\u0007]/.test(log), JSON.stringify(log));
  assert.match(log, /delta chart skipped: bad/);
});

test('setOutputs_should_use_a_heredoc_for_carriage_returns_and_redraw_the_delimiter_on_collision', () => {
  const crypto = require('node:crypto');
  const file = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'bd-out-')), 'output');
  const process_ = process.env.GITHUB_OUTPUT;
  process.env.GITHUB_OUTPUT = file;
  const orig = crypto.randomBytes;
  const seq = [Buffer.from('0000000000000001', 'hex'), Buffer.from('0000000000000002', 'hex')];
  crypto.randomBytes = () => seq.shift();
  try {
    setOutputs({ a: 'x\ry', b: 'benchdiff_0000000000000001\nz' });
  } finally {
    crypto.randomBytes = orig;
    if (process_ === undefined) delete process.env.GITHUB_OUTPUT; else process.env.GITHUB_OUTPUT = process_;
  }
  const text = fs.readFileSync(file, 'utf8');
  assert.ok(text.startsWith('a<<benchdiff_'), text);
  assert.ok(text.includes('b<<benchdiff_0000000000000002\nbenchdiff_0000000000000001\nz\nbenchdiff_0000000000000002\n'), text);
});

/** A workspace dir holding head.txt, with GITHUB_WORKSPACE pointed at it; fake-wasm friendly. */
function wsSetup(extra = {}) {
  const s = setup('');
  const dir = path.dirname(s.output);
  const ws = path.join(dir, 'ws');
  fs.mkdirSync(ws);
  fs.writeFileSync(path.join(ws, 'head.txt'), 'BenchmarkX-8 \t 1000 \t 1000.0 ns/op\n');
  delete s.env.INPUT_BASE;
  Object.assign(s.env, { INPUT_HEAD: path.join(ws, 'head.txt'), GITHUB_WORKSPACE: ws, ...extra });
  return { ...s, dir, ws };
}
const outsideFile = () => {
  const f = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'bd-outside-')), 'secret.txt');
  fs.writeFileSync(f, 'BenchmarkX-8 \t 1000 \t 1000.0 ns/op\n');
  return f;
};
const rejectsOutside = async (s, input) => withEnv(s.env, async () => {
  await assert.rejects(main({ runWasm: fakeWasm({ svgs: [] }).run }), new RegExp(`${input} '.*' resolves outside the workspace`));
});

test('main_should_reject_a_head_path_that_escapes_the_workspace_with_dotdot', async () => {
  const s = wsSetup();
  const out = outsideFile();
  s.env.INPUT_HEAD = path.relative(s.ws, out);
  await withEnv(s.env, async () => {
    const cwd = process.cwd();
    process.chdir(s.ws);
    try { await assert.rejects(main({ runWasm: fakeWasm({ svgs: [] }).run }), /head '.*secret\.txt' resolves outside the workspace/); } finally { process.chdir(cwd); }
  });
});

test('main_should_reject_a_head_symlink_that_points_outside_the_workspace', async () => {
  const s = wsSetup();
  fs.symlinkSync(outsideFile(), path.join(s.ws, 'link.txt'));
  s.env.INPUT_HEAD = path.join(s.ws, 'link.txt');
  await rejectsOutside(s, 'head');
});

test('main_should_reject_base_variant_and_config_paths_outside_the_workspace', async () => {
  const out = outsideFile();
  const base = wsSetup(); base.env.INPUT_BASE = out;
  await rejectsOutside(base, 'base');
  const cfg = wsSetup(); cfg.env.INPUT_CONFIG = out;
  await rejectsOutside(cfg, 'config');
  const variants = wsSetup(); delete variants.env.INPUT_HEAD;
  variants.env.INPUT_VARIANTS = `a=${path.join(variants.ws, 'head.txt')}\nb=${out}`;
  await rejectsOutside(variants, 'variants');
});

test('main_should_reject_an_svg_dir_outside_the_workspace_and_accept_one_inside', async () => {
  const outside = wsSetup({ INPUT_CHARTS: 'delta', 'INPUT_SVG-DIR': '../escape' });
  await rejectsOutside(outside, 'svg-dir');
  const inside = wsSetup({ INPUT_CHARTS: 'delta', 'INPUT_SVG-DIR': 'out/svgs' });
  await withEnv(inside.env, () => main({ runWasm: fakeWasm().run }));
  assert.ok(fs.existsSync(path.join(inside.ws, 'out/svgs/delta.svg')));
});

test('main_should_warn_in_log_when_svg_exceeds_100kb', async () => {
  const s = chartSetup();
  const big = `<svg xmlns="http://www.w3.org/2000/svg">${'a'.repeat(150 * 1024)}</svg>`;
  const fake = fakeWasm({ svgContent: big });
  const log = await runCaptured(s, fake, fakeGithub());
  assert.match(log, /^::warning::delta\.svg is 150 KB \(over the 100 KB guideline\)$/m);
  const small = await runCaptured(chartSetup(), fakeWasm(), fakeGithub());
  assert.ok(!/100 KB guideline/.test(small));
});

test('main_should_embed_when_charts_only_in_config_file_and_svg_embed_input_unset', async () => {
  const s = chartSetup({ INPUT_CHARTS: '' });
  const dir = path.dirname(s.output);
  fs.writeFileSync(path.join(dir, 'base.txt'), 'BenchmarkX-8 \t 1000 \t 1000.0 ns/op\n');
  fs.writeFileSync(path.join(dir, 'head.txt'), 'BenchmarkX-8 \t 1000 \t 1200.0 ns/op\n');
  fs.writeFileSync(path.join(dir, 'bd.config.json'), JSON.stringify({ version: 1, charts: 'delta' }));
  Object.assign(s.env, { INPUT_HEAD: path.join(dir, 'head.txt'), INPUT_BASE: path.join(dir, 'base.txt'), INPUT_CONFIG: path.join(dir, 'bd.config.json') });
  const gh = fakeGithub();
  const logs = [];
  const orig = process.stdout.write;
  process.stdout.write = (c) => { logs.push(String(c)); return true; };
  try { await withEnv(s.env, () => main({ fetchImpl: gh.fetchImpl })); } finally { process.stdout.write = orig; }
  assert.ok(gh.publishCalls().length > 0, 'charts from the config file were hosted');
  assert.ok(commentBody(gh).includes('raw.githubusercontent.com/o/r/'), 'comment embeds the hosted chart');
  assert.ok(!commentBody(gh).includes('benchdiff-svg:'));
  assert.match(logs.join(''), /^images: published 1 file to benchdiff-assets@/m);
});

test('ux_s4_5_should_log_the_publishing_line_before_the_first_git_request', async () => {
  const s = chartSetup();
  const gh = fakeGithub();
  const logs = [];
  let logLengthAtFirstGit = -1;
  const wrapped = (url, init) => {
    if (logLengthAtFirstGit < 0 && url.includes('/git/')) logLengthAtFirstGit = logs.join('').length;
    return gh.fetchImpl(url, init);
  };
  const orig = process.stdout.write;
  process.stdout.write = (c) => { logs.push(String(c)); return true; };
  try { await withEnv(s.env, () => main({ runWasm: fakeWasm().run, fetchImpl: wrapped })); } finally { process.stdout.write = orig; }
  assert.ok(logLengthAtFirstGit >= 0, 'a /git/ request was made');
  assert.match(logs.join('').slice(0, logLengthAtFirstGit), /^images: publishing to branch benchdiff-assets /m);
});

test('ci_workflow_should_run_wasm_fresh_on_tags_and_pass_event_data_through_env', () => {
  const wf = fs.readFileSync(path.join(root, '.github/workflows/ci.yml'), 'utf8');
  assert.match(wf, /^ {4}tags: \['v\*'\]$/m);
  assert.match(wf, /BASE_REF: \$\{\{ github\.base_ref \}\}/);
  assert.match(wf, /EVENT: \$\{\{ github\.event_name \}\}/);
  const run = wf.slice(wf.indexOf('touched dist/'));
  assert.ok(!/\$\{\{ github\.(base_ref|event_name) \}\}"/.test(run.split('env:')[1].split('run:')[1]), 'no expressions inside the script');
});
