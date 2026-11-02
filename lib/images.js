'use strict';
// Hosting seam (ADR-003): core emits `benchdiff-svg:NAME.svg` placeholders and never knows URLs;
// this module is the only place that publishes SVGs and rewrites or strips those placeholders.

// Must equal the Rust constant of the same name (asserted in test/images.test.js once it exists).
const PLACEHOLDER_SCHEME = 'benchdiff-svg:';

// Fixed error string per reason (plan Story 4.1.1; single source for ux.md).
const ERRORS = Object.freeze({
  private: 'private repository (no anonymous image URL)',
  fork: 'read-only token on a fork PR',
  permission: 'token lacks contents: write (grant it to host images)',
  'push-failed': 'could not update benchdiff-assets after 5 attempts',
  'unsupported-host': 'GitHub Enterprise host not supported for hosted images',
  'no-event': 'event payload unreadable',
  disabled: 'publish-images is false',
});

const crypto = require('node:crypto');

const BRANCH = 'benchdiff-assets';
const MAX_ATTEMPTS = 5;
const BACKOFF_BASE_MS = 250;

const sha256 = (data) => crypto.createHash('sha256').update(data).digest('hex');
const failure = (reason) => ({ error: ERRORS[reason], reason });
const defaultSleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Thin Git Data client; throws ApiError (status only, never the token or response body) on non-2xx.
class ApiError extends Error {
  constructor(status) {
    super(`GitHub API responded ${status}`);
    this.status = status;
  }
}

function apiClient(ctx) {
  const doFetch = ctx.fetch || globalThis.fetch;
  const base = `${(ctx.apiUrl || 'https://api.github.com').replace(/\/+$/, '')}/repos/${ctx.repository}/git`;
  return async (method, route, body, { allow404 = false } = {}) => {
    const res = await doFetch(`${base}${route}`, {
      method,
      headers: {
        Authorization: `Bearer ${ctx.token}`,
        Accept: 'application/vnd.github+json',
        'Content-Type': 'application/json',
        'X-GitHub-Api-Version': '2022-11-28',
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (allow404 && res.status === 404) return null;
    if (!res.ok) throw new ApiError(res.status);
    return res.json();
  };
}

/**
 * Publish SVG files to the orphan branch `benchdiff-assets` in one commit and return
 * `{urls: {name: rawUrl}}` or `{error, reason}`; never throws.
 * ctx: {token, repository: 'owner/repo', tag, fetch?, apiUrl?, rawBase?, sleep?, random?}.
 * Gating (private/fork/host) and no-op-on-identical-content are layered on by later tasks.
 */
async function publishImages(files, ctx = {}) {
  if (!ctx.token || !ctx.repository) return failure('disabled');
  try {
    return await pushFiles(files, ctx);
  } catch (_err) {
    return failure('push-failed'); // deliberately drops the error: messages may echo request details
  }
}

async function pushFiles(files, ctx) {
  const api = apiClient(ctx);
  const sleep = ctx.sleep || defaultSleep;
  const random = ctx.random || Math.random;
  const rawBase = (ctx.rawBase || 'https://raw.githubusercontent.com').replace(/\/+$/, '');
  const tagDir = `t-${sha256(ctx.tag || '').slice(0, 12)}`;

  // Blobs are content-addressed, so they are created once and reused across retries.
  const entries = [];
  for (const f of files) {
    const bytes = Buffer.from(f.bytes);
    const { sha } = await api('POST', '/blobs', { content: bytes.toString('base64'), encoding: 'base64' });
    entries.push({ name: f.name, sha, path: `${tagDir}/${sha256(bytes).slice(0, 8)}/${f.name}` });
  }

  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt += 1) {
    // Re-read the tip every attempt: it may exist (spike commit, concurrent run) or not (first run).
    const ref = await api('GET', `/ref/heads/${BRANCH}`, undefined, { allow404: true });
    const tip = ref ? ref.object.sha : null;
    const baseTree = tip ? (await api('GET', `/commits/${tip}`)).tree.sha : undefined;
    const tree = await api('POST', '/trees', {
      ...(baseTree ? { base_tree: baseTree } : {}),
      tree: entries.map((e) => ({ path: e.path, mode: '100644', type: 'blob', sha: e.sha })),
    });
    const commit = await api('POST', '/commits', {
      message: 'benchdiff: publish chart images',
      tree: tree.sha,
      parents: tip ? [tip] : [],
    });
    try {
      if (tip) await api('PATCH', `/refs/heads/${BRANCH}`, { sha: commit.sha, force: false });
      else await api('POST', '/refs', { ref: `refs/heads/${BRANCH}`, sha: commit.sha });
    } catch (err) {
      // 422: stale tip (not a fast-forward) or create race; loop re-reads and re-parents.
      if (!(err instanceof ApiError) || err.status !== 422) throw err;
      if (attempt < MAX_ATTEMPTS) await sleep(BACKOFF_BASE_MS * 2 ** (attempt - 1) * (1 + random()));
      continue;
    }
    const urls = {};
    for (const e of entries) urls[e.name] = `${rawBase}/${ctx.repository}/${commit.sha}/${e.path}`;
    return { urls };
  }
  return failure('push-failed');
}

const TOKEN = PLACEHOLDER_SCHEME.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const IMAGE_RE = new RegExp(`!\\[(?:\\\\.|[^\\]\\\\])*\\]\\(${TOKEN}([^)\\s]*)\\)`, 'g');
const PICTURE_RE = /<picture>[\s\S]*?<\/picture>/g;
const TOKEN_RE = new RegExp(`${TOKEN}([^"'\\s)<>]+)`, 'g');
const CHART_DETAILS_RE = /<details>(\s*<summary>Chart data \(text\))/g;

function failureLine(result, svgDir) {
  const parts = [`charts could not be hosted: ${result.error}`];
  if (result.reason === 'permission') {
    parts.push('add the permissions block from README "Hosting images"');
  }
  parts.push(`SVGs are in ${svgDir}`, 'to keep them, upload that directory with actions/upload-artifact');
  return `⚠️ ${parts.join('; ')}`;
}

// Put the warning after the sticky marker so the marker stays the first line.
function insertAfterMarker(body, line) {
  const nl = body.indexOf('\n');
  if (nl === -1 || !body.startsWith('<!--')) return `${line}\n\n${body}`;
  return `${body.slice(0, nl + 1)}\n${line}\n${body.slice(nl + 1)}`;
}

function tidyBlankRuns(s) {
  return s.replace(/\n{3,}/g, '\n\n');
}

/**
 * Rewrite placeholders to hosted URLs, or strip them with a warning.
 * Returns `{markdown, warning}`; `warning` is a string only when charts were stripped because the
 * publish failed (the same line is already in the markdown, so the caller also logs it via core.warning).
 */
function rewritePlaceholders(markdown, result, { svgDir = '<svg-dir>' } = {}) {
  const urls = result && result.urls;
  const hosted = (name) => (urls && Object.hasOwn(urls, name) ? urls[name] : undefined);
  let stripped = false;

  const fix = (block, tokenOf) => {
    const names = [...block.matchAll(TOKEN_RE)].map((m) => m[1]);
    if (names.length === 0 || !names.every((n) => hosted(n))) {
      stripped = true;
      return '\0STRIP\0';
    }
    return tokenOf(block);
  };
  const swap = (block) => block.replace(TOKEN_RE, (_m, name) => hosted(name));

  let out = markdown.replace(PICTURE_RE, (b) => (b.includes(PLACEHOLDER_SCHEME) ? fix(b, swap) : b));
  out = out.replace(IMAGE_RE, (b) => fix(b, swap));
  if (!stripped) return { markdown: out, warning: undefined };

  out = out.replace(/^[ \t]*\0STRIP\0[ \t]*\n?/gm, '').replace(/\0STRIP\0/g, '');
  out = tidyBlankRuns(out.replace(CHART_DETAILS_RE, '<details open>$1'));

  const failed = result && result.error ? result : { error: ERRORS.disabled, reason: 'disabled' };
  const warning = failureLine(failed, svgDir);
  return { markdown: insertAfterMarker(out, warning), warning };
}

module.exports = { PLACEHOLDER_SCHEME, ERRORS, publishImages, rewritePlaceholders };
