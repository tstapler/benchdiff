'use strict';
// GitHub Action entry point. Zero dependencies: runs the committed wasm build of the
// `benchdiff` CLI under node:wasi against a preopened temp dir, then publishes the result
// (outputs, job summary, sticky PR comment).

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { WASI } = require('node:wasi');

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
 */
function buildArgs({ head, base, config, format, baseFormat, threshold, minAbs, onlyChanged, sort, maxRows, title, tag, failOnRegression }) {
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
  fs.appendFileSync(file, Object.entries(values).map(([k, v]) => `${k}=${v}\n`).join(''));
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

async function main() {
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
  });

  const { code, stderr } = await runWasm(args, workDir);
  if (code !== 0 && code !== 1) throw new Error(stderr.trim() || `benchdiff exited with code ${code}`);

  const markdown = fs.readFileSync(path.join(workDir, 'report.md'), 'utf8');
  const report = JSON.parse(fs.readFileSync(path.join(workDir, 'report.json'), 'utf8'));
  // The binary owns the tag default and may take it from the config, so read it back from the report.
  const marker = markdown.split('\n', 1)[0];

  if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${markdown}\n`);

  if (input('comment', 'true') === 'true') {
    const result = await upsertComment({ token: input('github-token', process.env.GITHUB_TOKEN || ''), marker, body: markdown });
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
  });

  if (report.failed) throw new Error(`${report.counts.regressed} benchmark regression(s) exceed the fail-on-regression limit`);
}

const { publishImages, rewritePlaceholders } = require('./lib/images');

module.exports = { publishImages, rewritePlaceholders, expand, runWasm, stage, buildArgs, stageConfig, parseFailOnRegression, validateTag, upsertComment, clip, main };

if (require.main === module) {
  main().catch((e) => {
    process.stdout.write(`::error::${escapeCommand(e.message)}\n`);
    process.exitCode = 1;
  });
}
