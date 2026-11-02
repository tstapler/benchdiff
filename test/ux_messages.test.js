'use strict';
// AC-X-1: every warning/error the Action emits for charts and hosting names an exit path
// (a fix key, valid values, a location or a fallback view). Messages: design/ux.md S1 and S4.

const test = require('node:test');
const assert = require('node:assert/strict');
const { MESSAGES, PERMISSIONS_SNIPPET } = require('../index.js');
const { ERRORS, rewritePlaceholders } = require('../lib/images.js');

const EXIT_PATH = /(set |add |grant |check |upload|see README|falling back|remain in|SVGs are in|table is complete|leave )/;
const PLACEHOLDER_MD = '<!-- benchdiff:default -->\n## T\n\n![c](benchdiff-svg:delta.svg)\n\n<details>\n<summary>Chart data (text)</summary>\n</details>\n';

test('ux_x_1_every_warning_and_error_message_should_name_an_exit_path', () => {
  const warnings = {
    copyFailed: MESSAGES.copyFailed('out', 'EACCES', '/tmp/svg'),
    chartsFailed: MESSAGES.chartsFailed('boom'),
    cannotWrite: MESSAGES.cannotWrite('cannot write /work/svg: permission denied; report printed without charts'),
    notHosted: MESSAGES.notHosted('fork PR'),
    permissionSnippet: MESSAGES.permissionSnippet,
  };
  for (const [name, text] of Object.entries(warnings)) assert.match(text, EXIT_PATH, name);
  for (const reason of Object.keys(ERRORS)) {
    const { warning } = rewritePlaceholders(PLACEHOLDER_MD, { error: ERRORS[reason], reason }, { svgDir: '/tmp/svg' });
    assert.match(warning, /SVGs are in \/tmp\/svg; to keep them, upload that directory with actions\/upload-artifact/, reason);
    assert.ok(warning.startsWith('⚠️ charts could not be hosted: '), reason);
  }
});

test('ux_should_name_the_permissions_block_for_a_read_only_token', () => {
  const { warning, markdown } = rewritePlaceholders(PLACEHOLDER_MD, { error: ERRORS.permission, reason: 'permission' }, { svgDir: '/tmp/svg' });
  assert.ok(warning.includes('token lacks contents: write (grant it to host images); add the permissions block from README "Hosting images"'));
  assert.ok(markdown.includes('<details open>'));
  assert.ok(MESSAGES.permissionSnippet.endsWith(PERMISSIONS_SNIPPET));
  assert.match(PERMISSIONS_SNIPPET, /contents: write[\s\S]*pull-requests: write/);
});

test('ux_should_use_the_documented_log_lines', () => {
  assert.equal(MESSAGES.chartsLine('delta,ranking', 2, 1, ['distribution chart skipped: no samples in the input (use -count=10 or Criterion)']),
    'charts: requested=delta,ranking produced=2 skipped=1 (distribution chart skipped: no samples in the input (use -count=10 or Criterion))');
  assert.equal(MESSAGES.chartsLine('delta', 1, 0, []), 'charts: requested=delta produced=1 skipped=0');
  assert.equal(MESSAGES.published(2, 'abc'), 'images: published 2 files to benchdiff-assets@abc');
  assert.equal(MESSAGES.notHosted('fork PR'), 'images: not hosted (fork PR); falling back to Markdown');
  assert.equal(MESSAGES.publishing, 'images: publishing to branch benchdiff-assets (needs contents: write; commits are kept; see README "Hosting images")');
});

test('ux_should_never_put_a_token_in_any_message', () => {
  const all = [MESSAGES.publishing, MESSAGES.permissionSnippet, MESSAGES.notHosted('x'), MESSAGES.published(1, 'sha')];
  for (const m of all) assert.ok(!/ghs_|ghp_|Bearer/.test(m));
});
