'use strict';
// Unit tests for lib/images.js: the publishImages seam and placeholder rewrite/strip.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const images = require('../lib/images.js');
const index = require('../index.js');

const { PLACEHOLDER_SCHEME, ERRORS, publishImages, rewritePlaceholders } = images;
const root = path.resolve(__dirname, '..');
const MARKER = '<!-- benchdiff:default -->';
const URL_D = 'https://raw.githubusercontent.com/o/r/abc/x/delta.svg';
const DETAILS = '<details><summary>Chart data (text)</summary>\n\n| # | Benchmark |\n|---|---|\n| 1 | a |\n\n</details>';
const body = `${MARKER}\n## Benchmarks\n\n![Benchmark change](benchdiff-svg:delta.svg)\n\n${DETAILS}\n\nfooter\n`;

test('rewritePlaceholders_should_replace_placeholder_with_url_and_keep_marker_first_when_published', () => {
  const r = rewritePlaceholders(body, { urls: { 'delta.svg': URL_D } });
  assert.ok(r.markdown.startsWith(`${MARKER}\n`));
  assert.ok(r.markdown.includes(`![Benchmark change](${URL_D})`));
  assert.ok(!r.markdown.includes(PLACEHOLDER_SCHEME));
  assert.ok(r.markdown.includes('<details><summary>'), 'details stay collapsed when hosted');
  assert.equal(r.warning, undefined);
});

test('rewritePlaceholders_should_replace_every_benchdiff_svg_token_when_picture_block', () => {
  const pic = '<picture><source media="(prefers-color-scheme: dark)" srcset="benchdiff-svg:delta-dark.svg"><img alt="A &amp; B" src="benchdiff-svg:delta-light.svg"></picture>';
  const md = `${MARKER}\n\n${pic}\n\n${DETAILS}\n`;
  const urls = { 'delta-dark.svg': 'https://h/d.svg', 'delta-light.svg': 'https://h/l.svg' };
  const r = rewritePlaceholders(md, { urls });
  assert.ok(r.markdown.includes('srcset="https://h/d.svg"'));
  assert.ok(r.markdown.includes('src="https://h/l.svg"'));
  assert.ok(!r.markdown.includes(PLACEHOLDER_SCHEME));
});

test('rewritePlaceholders_should_strip_image_and_warn_when_publish_fails', () => {
  const r = rewritePlaceholders(body, { error: ERRORS.fork, reason: 'fork' }, { svgDir: 'out/svg' });
  assert.ok(r.markdown.startsWith(`${MARKER}\n`));
  assert.ok(!r.markdown.includes(PLACEHOLDER_SCHEME));
  assert.ok(!r.markdown.includes('![Benchmark change]'));
  assert.ok(!/\n{3,}/.test(r.markdown), 'no blank-line runs left behind');
  assert.equal(
    r.warning,
    '⚠️ charts could not be hosted: read-only token on a fork PR; SVGs are in out/svg; to keep them, upload that directory with actions/upload-artifact',
  );
  assert.ok(r.markdown.includes(r.warning), 'warning is in the body, not only returned');
});

test('rewritePlaceholders_should_strip_whole_picture_block_without_dangling_markup_when_publish_fails', () => {
  const pic = '<picture><source srcset="benchdiff-svg:a-dark.svg">\n<img alt="x" src="benchdiff-svg:a-light.svg"></picture>';
  const r = rewritePlaceholders(`${MARKER}\n\n${pic}\n\n${DETAILS}\n`, { error: ERRORS.private, reason: 'private' });
  assert.ok(!/<picture|<\/picture|<source|<img/.test(r.markdown));
  assert.ok(r.markdown.includes('<details open>'));
});

test('rewritePlaceholders_should_open_chart_data_details_and_put_warning_in_markdown_body_when_publish_fails', () => {
  const two = `${MARKER}\n![a](benchdiff-svg:r.svg)\n<details><summary>Chart data (text): ranking</summary>\nx\n</details>\n<details><summary>3 unchanged</summary>\ny\n</details>\n`;
  const r = rewritePlaceholders(two, { error: ERRORS.fork, reason: 'fork' });
  assert.ok(r.markdown.includes('<details open><summary>Chart data (text): ranking</summary>'));
  assert.ok(r.markdown.includes('<details><summary>3 unchanged</summary>'), 'other details untouched');
  assert.ok(r.markdown.includes('⚠️ charts could not be hosted'));
});

test('rewritePlaceholders_should_name_directory_and_upload_artifact_hint_and_add_permissions_pointer_when_permission', () => {
  const r = rewritePlaceholders(body, { error: ERRORS.permission, reason: 'permission' }, { svgDir: '/tmp/svg' });
  assert.equal(
    r.warning,
    '⚠️ charts could not be hosted: token lacks contents: write (grant it to host images); add the permissions block from README "Hosting images"; SVGs are in /tmp/svg; to keep them, upload that directory with actions/upload-artifact',
  );
  assert.ok(!/artifact name/i.test(r.warning));
});

test('rewritePlaceholders_should_strip_a_block_whose_url_is_missing_when_urls_partial', () => {
  const md = `${MARKER}\n![a](benchdiff-svg:a.svg)\n![b](benchdiff-svg:b.svg)\n`;
  const r = rewritePlaceholders(md, { urls: { 'a.svg': 'https://h/a.svg' } });
  assert.ok(r.markdown.includes('![a](https://h/a.svg)'));
  assert.ok(!r.markdown.includes('![b]'));
  assert.ok(r.warning);
});

test('rewritePlaceholders_should_leave_body_unchanged_when_no_placeholders', () => {
  const md = `${MARKER}\nplain table\n`;
  assert.equal(rewritePlaceholders(md, { error: ERRORS.disabled, reason: 'disabled' }).markdown, md);
  assert.equal(rewritePlaceholders(md, { error: ERRORS.disabled, reason: 'disabled' }).warning, undefined);
});

test('rewritePlaceholders_should_not_interpret_dollar_sequences_when_url_contains_them', () => {
  const r = rewritePlaceholders(`${MARKER}\n![a](benchdiff-svg:a.svg)\n`, { urls: { 'a.svg': 'https://h/$&$1.svg' } });
  assert.ok(r.markdown.includes('(https://h/$&$1.svg)'));
});

test('publishImages_should_return_documented_result_shape_and_never_throw_when_noop', async () => {
  const r = await publishImages([{ name: 'delta.svg', bytes: Buffer.from('<svg/>') }], {});
  assert.deepEqual(Object.keys(r).sort(), ['error', 'reason']);
  assert.equal(r.error, ERRORS[r.reason]);
});

test('errors_should_define_exactly_the_seven_reasons_with_fixed_strings', () => {
  assert.deepEqual(Object.keys(ERRORS).sort(), ['disabled', 'fork', 'no-event', 'permission', 'private', 'push-failed', 'unsupported-host']);
});

test('index_should_still_export_existing_helpers_and_the_images_seam', () => {
  for (const k of ['runWasm', 'stage', 'expand', 'upsertComment', 'clip']) assert.equal(typeof index[k], 'function', k);
  assert.equal(index.publishImages, publishImages);
  assert.equal(index.rewritePlaceholders, rewritePlaceholders);
});

test('images_module_should_require_only_node_builtins_when_loaded', () => {
  const src = fs.readFileSync(path.join(root, 'lib', 'images.js'), 'utf8');
  const specs = [...src.matchAll(/require\(\s*['"]([^'"]+)['"]\s*\)|from\s+['"]([^'"]+)['"]/g)].map((m) => m[1] || m[2]);
  for (const s of specs) assert.ok(s.startsWith('node:') || s.startsWith('./') || s.startsWith('../'), s);
});

test('placeholder_scheme_should_equal_rust_constant_when_it_exists', (t) => {
  const found = [];
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith('.rs')) {
        const m = fs.readFileSync(p, 'utf8').match(/const\s+PLACEHOLDER_SCHEME\s*:\s*&(?:'static\s+)?str\s*=\s*"([^"]*)"/);
        if (m) found.push(m[1]);
      }
    }
  };
  walk(path.join(root, 'crates'));
  if (found.length === 0) return t.skip('Rust PLACEHOLDER_SCHEME not yet defined (arrives in 3.3.1c); cross-check pending');
  assert.deepEqual(found, [PLACEHOLDER_SCHEME]);
});
