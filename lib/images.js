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

/**
 * Publish SVG files and return `{urls: {name: url}}` or `{error, reason}`; never throws.
 * Placeholder implementation: hosts nothing, so callers always take the strip path until the
 * orphan-branch publisher (4.1.2) replaces this body.
 */
async function publishImages(_files, _ctx) {
  return { error: ERRORS.disabled, reason: 'disabled' };
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
