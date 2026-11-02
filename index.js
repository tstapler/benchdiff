'use strict';
// GitHub Action entry point. Zero dependencies: runs the committed wasm build of the
// `benchdiff` CLI under node:wasi against a preopened temp dir, then publishes the result
// (outputs, job summary, sticky PR comment).

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { WASI } = require('node:wasi');

const { PLACEHOLDER_SCHEME, publishImages, rewritePlaceholders } = require('./lib/images');

const WASM_PATH = path.join(__dirname, 'dist', 'benchdiff.wasm');
const COMMENT_LIMIT = 60000; // GitHub rejects comment bodies over 65536 characters.

const escapeCommand = (s) => String(s).replace(/%/g, '%25').replace(/\r/g, '%0D').replace(/\n/g, '%0A');
const warning = (msg) => process.stdout.write(`::warning::${escapeCommand(msg)}\n`);
const notice = (msg) => process.stdout.write(`::notice::${escapeCommand(msg)}\n`);

function input(name, fallback = '') {
  const v = process.env[`INPUT_${name.toUpperCase()}`];
  return v === undefined || v.trim() === '' ? fallback : v.trim();
}

/** Newline/comma separated paths and globs -> existing files, in order, de-duplicated. */
function expand(patterns, cwd = process.cwd()) {
  const files = [];
  for (const pattern of patterns.split(/[\n,]/).map((p) => p.trim()).filter(Boolean)) {
    const direct = path.resolve(cwd, pattern);
    const matches = fs.existsSync(direct) && fs.statSync(direct).isFile()
      ? [direct]
      : fs.globSync(pattern, { cwd }).map((m) => path.resolve(cwd, m)).filter((m) => fs.statSync(m).isFile()).sort();
    for (const m of matches) if (!files.includes(m)) files.push(m);
  }
  return files;
}

/** Runs the wasm CLI with `args` (paths under /work) and returns its exit code and stderr. */
async function runWasm(args, workDir) {
  const errPath = path.join(workDir, 'stderr.txt');
  const outPath = path.join(workDir, 'stdout.txt');
  const errFd = fs.openSync(errPath, 'w');
  const outFd = fs.openSync(outPath, 'w');
  try {
    const wasi = new WASI({
      version: 'preview1',
      args: ['benchdiff', ...args],
      env: {},
      preopens: { '/work': workDir },
      stdout: outFd,
      stderr: errFd,
      returnOnExit: true,
    });
    const module = await WebAssembly.compile(fs.readFileSync(WASM_PATH));
    const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
    let code;
    try {
      code = wasi.start(instance);
    } catch (e) {
      if (!(e instanceof WebAssembly.RuntimeError)) throw e;
      const stderr = fs.readFileSync(errPath, 'utf8').trim();
      throw new Error(`benchdiff wasm trapped: ${e.message}${stderr ? `\n${stderr}` : ''}`);
    }
    return { code, stderr: fs.readFileSync(errPath, 'utf8'), stdout: fs.readFileSync(outPath, 'utf8') };
  } finally {
    fs.closeSync(errFd);
    fs.closeSync(outFd);
  }
}

/** Copies `files` into `dir` under unique names and returns the matching /work paths. */
function stage(files, dir, prefix) {
  return files.map((file, i) => {
    const name = `${prefix}-${i}-${path.basename(file)}`;
    fs.copyFileSync(file, path.join(dir, name));
    return `/work/${name}`;
  });
}

/**
 * Builds the CLI argv. Config-governed settings are emitted only when the raw input is
 * non-empty, so a `--config` file (or the binary's own default) applies when the user set nothing.
 * The one exception is `svgEmbed`: the Action passes `auto` when it is '' (unset), so `charts` alone
 * embeds even when it comes from the config file. `undefined` means the caller is not wired for
 * charts yet (main() until task 4.2.1b), and nothing is emitted.
 * `dimensions` is newline-delimited and becomes one `--dimensions` flag per non-blank line.
 */
function buildArgs({ head, base, config, format, baseFormat, threshold, minAbs, onlyChanged, sort, maxRows, title, tag, failOnRegression, compareBy, groupBy, dimensions, dimensionsStrict, reference, correction, cellFormat, charts, chartRows, chartWidth, chartTheme, svgDir, svgEmbed, publishImages }) {
  const args = [];
  head.forEach((f) => args.push('--head', f));
  base.forEach((f) => args.push('--base', f));
  if (config) args.push('--config', config);
  if (format) args.push('--format', format);
  if (threshold) args.push('--threshold', threshold);
  if (minAbs) args.push('--min-abs', minAbs);
  if (sort) args.push('--sort', sort);
  if (title) args.push(`--title=${title}`);
  if (tag) args.push(`--tag=${tag}`);
  if (baseFormat) args.push('--base-format', baseFormat);
  if (onlyChanged === 'true') args.push('--only-changed');
  else if (onlyChanged === 'false') args.push('--no-only-changed');
  if (maxRows) args.push('--max-rows', maxRows);
  if (failOnRegression === 'true') args.push('--fail-on-regression');
  else if (failOnRegression === 'false') args.push('--no-fail-on-regression');
  else if (failOnRegression) args.push(`--fail-on-regression=${failOnRegression}`);
  if (compareBy) args.push('--compare-by', compareBy);
  if (groupBy) args.push('--group-by', groupBy);
  String(dimensions ?? '').split('\n').map((l) => l.trim()).filter(Boolean).forEach((rule) => args.push('--dimensions', rule));
  if (dimensionsStrict === 'true') args.push('--dimensions-strict');
  if (reference) args.push('--reference', reference);
  if (correction) args.push('--correction', correction);
  if (cellFormat) args.push('--cell-format', cellFormat);
  if (charts) args.push('--charts', charts);
  if (chartRows) args.push('--chart-rows', chartRows);
  if (chartWidth) args.push('--chart-width', chartWidth);
  if (chartTheme) args.push('--chart-theme', chartTheme);
  if (svgDir) args.push('--svg-dir', svgDir);
  if (svgEmbed !== undefined) args.push(`--svg-embed=${svgEmbed || 'auto'}`);
  if (publishImages) args.push(`--publish-images=${publishImages}`);
  args.push('--markdown', '/work/report.md', '--json', '/work/report.json', '--head-json', '/work/head.json');
  return args;
}

/** Copies the explicit config file to `<dir>/config/<its own base name>` so errors name the file the user wrote. */
function stageConfig(file, dir, cwd = process.cwd()) {
  const source = path.resolve(cwd, file);
  if (!fs.existsSync(source) || !fs.statSync(source).isFile()) throw new Error(`config file not found: ${file}`);
  const name = path.basename(source).replace(/[^A-Za-z0-9._-]/g, '_');
  fs.mkdirSync(path.join(dir, 'config'), { recursive: true });
  fs.copyFileSync(source, path.join(dir, 'config', name));
  return `/work/config/${name}`;
}

/** Normalizes the fail-on-regression input to '' (unset), 'true', 'false' or a non-negative number string. */
function parseFailOnRegression(raw) {
  const v = String(raw ?? '').trim().toLowerCase();
  if (v === '') return '';
  if (v === 'false') return 'false';
  if (v === 'true') return 'true';
  if (!/^\d+(\.\d+)?$/.test(v)) throw new Error(`fail-on-regression must be true, false or a number >= 0, got ${JSON.stringify(raw)}`);
  return v;
}

/** Normalizes a boolean input to '' (unset), 'true' or 'false'. */
function parseBoolInput(name, raw) {
  const v = String(raw ?? '').trim().toLowerCase();
  if (v === '' || v === 'true' || v === 'false') return v;
  throw new Error(`${name} must be true or false, got ${JSON.stringify(raw)}`);
}

/** The tag is embedded in an HTML comment; the same charset as the Rust `marker()` keeps both markers identical. */
function validateTag(tag) {
  if (!/^[\p{L}\p{N}\-_.:/]+$/u.test(tag)) {
    throw new Error(`tag may only contain letters, digits and - _ . : /, got ${JSON.stringify(tag)}`);
  }
  return tag;
}

function setOutputs(values) {
  const file = process.env.GITHUB_OUTPUT;
  if (!file) return;
  const line = ([k, v]) => {
    const text = String(v);
    if (!text.includes('\n')) return `${k}=${text}\n`;
    const delim = `benchdiff_${require('node:crypto').randomBytes(8).toString('hex')}`;
    return `${k}<<${delim}\n${text}\n${delim}\n`;
  };
  fs.appendFileSync(file, Object.entries(values).map(line).join(''));
}

function pullRequestNumber() {
  try {
    const event = JSON.parse(fs.readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
    return event.pull_request?.number ?? (event.issue?.pull_request ? event.issue.number : undefined);
  } catch {
    return undefined;
  }
}

// Moves `cut` before any image or <picture> span it would split, so no broken markup is posted.
function beforeImageMarkup(body, cut) {
  const spans = /<picture>[\s\S]*?<\/picture>|!\[(?:\\.|[^\]\\])*\]\([^)\s]*\)/g;
  for (const m of body.matchAll(spans)) {
    if (m.index >= cut) break;
    if (m.index + m[0].length > cut) return m.index;
  }
  return cut;
}

function clip(body, marker) {
  if (body.length <= COMMENT_LIMIT) return body;
  const cut = body.lastIndexOf('\n', COMMENT_LIMIT);
  const kept = body.slice(0, beforeImageMarkup(body, cut > 0 ? cut : COMMENT_LIMIT));
  // A cut inside <details> would swallow the footer and marker into the collapsed block.
  const open = Math.max(0, (kept.match(/<details>/g) || []).length - (kept.match(/<\/details>/g) || []).length);
  const closers = '\n</details>'.repeat(open);
  return `${kept}${closers}\n\n_Report truncated to fit GitHub's comment limit; the job summary has the full text._\n${marker}\n`;
}

/** Creates or updates the one comment carrying `marker`. Returns what happened, never throws. */
async function upsertComment({ token, marker, body, fetchImpl = fetch, env = process.env, number = pullRequestNumber() }) {
  if (!number) return { status: 'skipped', reason: 'not a pull request event' };
  const api = env.GITHUB_API_URL || 'https://api.github.com';
  const base = `${api}/repos/${env.GITHUB_REPOSITORY}/issues`;
  const headers = {
    authorization: `Bearer ${token}`,
    accept: 'application/vnd.github+json',
    'x-github-api-version': '2022-11-28',
    'content-type': 'application/json',
    'user-agent': 'benchdiff-action',
  };
  const call = async (url, init = {}) => {
    const res = await fetchImpl(url, { ...init, headers });
    if (!res.ok) {
      const err = new Error(`${init.method || 'GET'} ${url.replace(api, '')} -> ${res.status}`);
      err.status = res.status;
      throw err;
    }
    return res.json();
  };
  try {
    let existing;
    for (let page = 1; !existing; page++) {
      const batch = await call(`${base}/${number}/comments?per_page=100&page=${page}`);
      // Bot-authored and marker-first, so a user quoting the marker is never overwritten.
      existing = batch.find((c) => c.user?.type === 'Bot' && typeof c.body === 'string' && c.body.startsWith(marker));
      if (batch.length < 100) break;
    }
    const payload = JSON.stringify({ body: clip(body, marker) });
    if (existing) {
      await call(`${base}/comments/${existing.id}`, { method: 'PATCH', body: payload });
      return { status: 'updated', id: existing.id };
    }
    const created = await call(`${base}/${number}/comments`, { method: 'POST', body: payload });
    return { status: 'created', id: created.id };
  } catch (e) {
    const readOnly = e.status === 401 || e.status === 403;
    return { status: 'failed', readOnly, reason: e.message };
  }
}


// User-facing chart and hosting messages (design/ux.md). Each names its exit path (AC-X-1);
// test/ux_messages.test.js asserts that.
const PERMISSIONS_SNIPPET = 'permissions:\n  contents: write        # host chart images on the benchdiff-assets branch\n  pull-requests: write   # sticky comment';
const MESSAGES = Object.freeze({
  publishing: 'images: publishing to branch benchdiff-assets (needs contents: write; commits are kept; see README "Hosting images")',
  published: (n, sha) => `images: published ${n} file${n === 1 ? '' : 's'} to benchdiff-assets@${sha}`,
  notHosted: (why) => `images: not hosted (${why}); falling back to Markdown`,
  permissionSnippet: `images: to host charts, add this to the job or workflow (README "Hosting images"):\n${PERMISSIONS_SNIPPET}`,
  chartsLine: (requested, produced, skipped, reasons) => `charts: requested=${requested} produced=${produced} skipped=${skipped}${reasons.length ? ` (${reasons.join('; ')})` : ''}`,
  copyFailed: (dir, cause, kept) => `charts: could not copy SVG files to ${dir} (${cause}); they remain in ${kept}; set svg-dir to a writable workspace path`,
  chartsFailed: (cause) => `charts: could not finish the chart step (${cause}); the table is complete; check the charts input or set charts to none`,
  cannotWrite: (detail) => `${detail}; the table is complete; set svg-dir to a writable path or leave charts unset`,
});
const HOST_LABELS = { fork: 'fork PR', private: 'private repository', permission: 'token lacks contents: write', disabled: 'publish-images is false' };

const isPlaceholderRun = (markdown) => markdown.includes(PLACEHOLDER_SCHEME);
const logLine = (msg) => process.stdout.write(`${msg}\n`);
const tagOfMarker = (marker) => (/^<!-- benchdiff:(.*) -->$/.exec(marker) || [])[1] || '';

/** Parses the binary's stderr: its own produced/skipped line and the "chart skipped" notices. */
function chartSummary(stderr, fileCount) {
  const own = /charts: produced=(\d+) skipped=(\d+)/.exec(stderr);
  const reasons = stderr.split('\n').map((l) => l.replace(/^notice: /, '').trim()).filter((l) => /chart skipped/.test(l));
  return { produced: own ? Number(own[1]) : fileCount, skipped: own ? Number(own[2]) : reasons.length, reasons };
}

/** Copies the produced SVGs where upload-artifact can reach them; returns {dir, paths}. */
function exposeSvgs(workSvg, names, wanted) {
  if (!names.length) return { dir: '', paths: [] };
  if (wanted) {
    const dest = path.resolve(process.env.GITHUB_WORKSPACE || process.cwd(), wanted);
    try {
      fs.mkdirSync(dest, { recursive: true });
      for (const n of names) fs.copyFileSync(path.join(workSvg, n), path.join(dest, n));
      return { dir: dest, paths: names.map((n) => path.join(dest, n)) };
    } catch (e) {
      warning(MESSAGES.copyFailed(wanted, e.code || e.message, workSvg));
    }
  }
  return { dir: workSvg, paths: names.map((n) => path.join(workSvg, n)) };
}

/** Hosts the SVGs when the Markdown carries placeholders, then rewrites or strips them. Never throws. */
async function embedCharts({ markdown, workSvg, names, svgDir, marker, fetchImpl }) {
  let announced = false;
  const announcingFetch = (...a) => {
    if (!announced) { announced = true; logLine(MESSAGES.publishing); }
    return fetchImpl(...a);
  };
  const env = process.env;
  let result;
  try {
    const files = names.map((name) => ({ name: name.replace(/\.svg$/, '') + '.svg', bytes: fs.readFileSync(path.join(workSvg, name)) }));
    result = await publishImages(files, {
      token: input('github-token', env.GITHUB_TOKEN || ''),
      repository: env.GITHUB_REPOSITORY,
      tag: tagOfMarker(marker),
      enabled: input('publish-images') !== 'false',
      eventName: env.GITHUB_EVENT_NAME,
      eventPath: env.GITHUB_EVENT_PATH,
      serverUrl: env.GITHUB_SERVER_URL,
      apiUrl: env.GITHUB_API_URL,
      fetch: announcingFetch,
    });
  } catch (e) {
    result = { error: `could not read the SVG files (${e.code || e.message})`, reason: 'push-failed' };
  }
  const rewritten = rewritePlaceholders(markdown, result, { svgDir });
  if (result.urls) {
    const sha = (Object.values(result.urls)[0] || '').split('/')[5] || 'unknown';
    logLine(MESSAGES.published(Object.keys(result.urls).length, sha));
  } else {
    logLine(MESSAGES.notHosted(HOST_LABELS[result.reason] || result.error));
    if (result.reason === 'permission') logLine(MESSAGES.permissionSnippet);
  }
  if (rewritten.warning) warning(rewritten.warning);
  return rewritten.markdown;
}

async function main({ runWasm: run = runWasm, fetchImpl = fetch } = {}) {
  const head = expand(input('head'));
  if (!head.length) throw new Error(`no head files match ${JSON.stringify(input('head'))}`);
  const basePattern = input('base');
  const base = basePattern ? expand(basePattern) : [];
  if (basePattern && !base.length) warning(`no base files match ${JSON.stringify(basePattern)}; reporting head only`);

  const tagInput = input('tag');
  const tag = tagInput ? validateTag(tagInput) : '';
  const failOnRegression = parseFailOnRegression(input('fail-on-regression'));
  const configInput = input('config');
  const workDir = fs.mkdtempSync(path.join(process.env.RUNNER_TEMP || os.tmpdir(), 'benchdiff-'));
  // charts may also come from the config file, whose content is unknown here: treat a config as "maybe charts".
  const chartsInput = input('charts');
  const chartsActive = (chartsInput !== '' && chartsInput.toLowerCase() !== 'none') || configInput !== '';
  // node:wasi cannot create_dir_all, so the directory must exist before the binary writes into it.
  if (chartsActive) fs.mkdirSync(path.join(workDir, 'svg'), { recursive: true });
  const args = buildArgs({
    head: stage(head, workDir, 'head'),
    base: stage(base, workDir, 'base'),
    config: configInput ? stageConfig(configInput, workDir) : '',
    format: input('format'),
    baseFormat: input('base-format'),
    threshold: input('threshold'),
    minAbs: input('min-abs'),
    onlyChanged: parseBoolInput('only-changed', input('only-changed')),
    sort: input('sort'),
    maxRows: input('max-rows'),
    title: input('title'),
    tag,
    failOnRegression,
    compareBy: input('compare-by'),
    groupBy: input('group-by'),
    dimensions: input('dimensions'),
    dimensionsStrict: parseBoolInput('dimensions-strict', input('dimensions-strict')),
    reference: input('reference'),
    correction: input('correction'),
    cellFormat: input('cell-format'),
    charts: chartsInput,
    chartRows: input('chart-rows'),
    chartWidth: input('chart-width'),
    chartTheme: input('chart-theme'),
    svgDir: chartsActive ? '/work/svg' : '',
    svgEmbed: chartsActive ? input('svg-embed') : undefined,
    publishImages: input('publish-images'),
  });

  const { code, stderr } = await run(args, workDir);
  const reportWritten = fs.existsSync(path.join(workDir, 'report.md'));
  const softChartError = code === 2 && reportWritten && /report printed without charts/.test(stderr);
  if (softChartError) warning(MESSAGES.cannotWrite(stderr.trim().split('\n').pop()));
  else if (code !== 0 && code !== 1) throw new Error(stderr.trim() || `benchdiff exited with code ${code}`);

  let markdown = fs.readFileSync(path.join(workDir, 'report.md'), 'utf8');
  const report = JSON.parse(fs.readFileSync(path.join(workDir, 'report.json'), 'utf8'));
  // The binary owns the tag default and may take it from the config, so read it back from the report.
  const marker = markdown.split('\n', 1)[0];

  const chartOutputs = {};
  if (chartsActive) {
    try {
      const workSvg = path.join(workDir, 'svg');
      const names = fs.existsSync(workSvg) ? fs.readdirSync(workSvg).filter((n) => n.endsWith('.svg')).sort() : [];
      const summary = chartSummary(stderr, names.length);
      logLine(MESSAGES.chartsLine(chartsInput || 'config', summary.produced, summary.skipped, summary.reasons));
      const exposed = exposeSvgs(workSvg, names, input('svg-dir'));
      if (isPlaceholderRun(markdown)) {
        markdown = await embedCharts({ markdown, workSvg, names, svgDir: exposed.dir || workSvg, marker, fetchImpl });
      }
      chartOutputs['svg-dir'] = exposed.dir;
      chartOutputs['svg-paths'] = exposed.paths.join('\n');
    } catch (e) {
      warning(MESSAGES.chartsFailed(e.message));
      if (isPlaceholderRun(markdown)) markdown = rewritePlaceholders(markdown, { error: e.message, reason: 'push-failed' }).markdown;
    }
  }

  if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${markdown}\n`);

  if (input('comment', 'true') === 'true') {
    const result = await upsertComment({ token: input('github-token', process.env.GITHUB_TOKEN || ''), marker, body: markdown, fetchImpl });
    if (result.status === 'failed') {
      warning(result.readOnly
        ? `Could not post the PR comment (${result.reason}): the token is read-only, as on pull requests from forks. The report is in the job summary.`
        : `Could not post the PR comment: ${result.reason}`);
    } else if (result.status === 'skipped') {
      notice(`PR comment skipped: ${result.reason}`);
    }
  }

  setOutputs({
    regressed: report.counts.regressed,
    improved: report.counts.improved,
    unchanged: report.counts.unchanged,
    new: report.counts.new,
    removed: report.counts.removed,
    failed: report.failed,
    'report-path': path.join(workDir, 'report.md'),
    'json-path': path.join(workDir, 'report.json'),
    'head-json-path': path.join(workDir, 'head.json'),
    ...chartOutputs,
  });

  if (report.failed) throw new Error(`${report.counts.regressed} benchmark regression(s) exceed the fail-on-regression limit`);
}

module.exports = { MESSAGES, PERMISSIONS_SNIPPET, publishImages, rewritePlaceholders, expand, runWasm, stage, buildArgs, stageConfig, parseFailOnRegression, validateTag, upsertComment, clip, main };

if (require.main === module) {
  main().catch((e) => {
    process.stdout.write(`::error::${escapeCommand(e.message)}\n`);
    process.exitCode = 1;
  });
}
