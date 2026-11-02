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

// ---- 4.1.2a: Git Data API client (mocked fetch only) ----
const crypto = require('node:crypto');
const sha = (s) => crypto.createHash('sha256').update(s).digest('hex');
const SVG = Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"/>');
const FILES = [{ name: 'delta.svg', bytes: SVG }];
const TIP = 'a'.repeat(40);
const TIP_TREE = 'b'.repeat(40);

// Minimal fake of the Git Data endpoints. `refReplies` scripts successive ref writes (PATCH/POST).
function fakeGitHub({ tip = TIP, refReplies = [] } = {}) {
  const calls = [];
  let n = 0;
  const json = (status, body) => ({ ok: status < 400, status, json: async () => body, text: async () => JSON.stringify(body) });
  const fetch = async (url, init = {}) => {
    const method = init.method || 'GET';
    const body = init.body ? JSON.parse(init.body) : undefined;
    calls.push({ url, method, body, headers: init.headers });
    const u = new URL(url);
    if (method === 'GET' && u.pathname.endsWith('/git/ref/heads/benchdiff-assets')) {
      return tip ? json(200, { object: { sha: tip } }) : json(404, { message: 'Not Found' });
    }
    if (method === 'GET' && u.pathname.includes('/git/commits/')) return json(200, { tree: { sha: TIP_TREE } });
    if (method === 'POST' && u.pathname.endsWith('/git/blobs')) return json(201, { sha: `blob${++n}`.padEnd(40, '0') });
    if (method === 'POST' && u.pathname.endsWith('/git/trees')) return json(201, { sha: 'c'.repeat(40) });
    if (method === 'POST' && u.pathname.endsWith('/git/commits')) return json(201, { sha: `d${calls.length}`.padEnd(40, '0') });
    if (u.pathname.includes('/git/refs')) {
      const r = refReplies.shift() || [200, {}];
      return json(r[0], r[1]);
    }
    return json(500, { message: 'unexpected' });
  };
  return { fetch, calls };
}
const mkCtx = (extra = {}) => ({
  token: 'ghs_SECRET_TOKEN',
  repository: 'o/r',
  tag: 'default',
  eventName: 'pull_request',
  event: { repository: { private: false }, pull_request: { head: { repo: { full_name: 'o/r' } }, base: { repo: { full_name: 'o/r' } } } },
  apiUrl: 'https://api.github.com',
  rawBase: 'https://raw.githubusercontent.com',
  sleep: async () => {},
  random: () => 0,
  ...extra,
});
const refWrites = (calls) => calls.filter((c) => c.method !== 'GET' && new URL(c.url).pathname.includes('/git/refs'));

test('publishImages_should_commit_on_existing_tip_and_return_sha_pinned_raw_url_when_branch_exists', async () => {
  const gh = fakeGitHub();
  const r = await publishImages(FILES, mkCtx({ fetch: gh.fetch }));
  const commitCall = gh.calls.find((c) => c.method === 'POST' && c.url.endsWith('/git/commits'));
  assert.deepEqual(commitCall.body.parents, [TIP], 'parented on the existing tip');
  const treeCall = gh.calls.find((c) => c.url.endsWith('/git/trees'));
  assert.equal(treeCall.body.base_tree, TIP_TREE, 'previous entries preserved via base_tree');
  const [write] = refWrites(gh.calls);
  assert.equal(write.method, 'PATCH');
  assert.equal(write.body.force, false, 'never force-push');
  const hash = sha(SVG).slice(0, 8);
  const p = `t-${sha('default').slice(0, 12)}/${hash}/delta.svg`;
  assert.equal(treeCall.body.tree[0].path, p);
  assert.deepEqual(r, { urls: { 'delta.svg': `https://raw.githubusercontent.com/o/r/${write.body.sha}/${p}` } });
});

test('publishImages_should_send_base64_blob_with_bearer_token_to_api_host_only_and_never_leak_token_when_publishing', async () => {
  const gh = fakeGitHub();
  const r = await publishImages(FILES, mkCtx({ fetch: gh.fetch }));
  const blob = gh.calls.find((c) => c.url.endsWith('/git/blobs'));
  assert.deepEqual(blob.body, { content: SVG.toString('base64'), encoding: 'base64' });
  for (const c of gh.calls) {
    assert.equal(new URL(c.url).host, 'api.github.com');
    assert.equal(c.headers.Authorization, 'Bearer ghs_SECRET_TOKEN');
  }
  assert.ok(!JSON.stringify(r).includes('SECRET'));
});

test('publishImages_should_create_orphan_root_commit_and_post_ref_when_branch_missing', async () => {
  const gh = fakeGitHub({ tip: null, refReplies: [[201, {}]] });
  const r = await publishImages(FILES, mkCtx({ fetch: gh.fetch }));
  const commitCall = gh.calls.find((c) => c.url.endsWith('/git/commits'));
  assert.deepEqual(commitCall.body.parents, []);
  assert.equal(gh.calls.find((c) => c.url.endsWith('/git/trees')).body.base_tree, undefined);
  const [write] = refWrites(gh.calls);
  assert.equal(write.method, 'POST');
  assert.equal(write.body.ref, 'refs/heads/benchdiff-assets');
  assert.ok(r.urls['delta.svg'].includes(`/o/r/${write.body.sha}/`));
});

test('publishImages_should_reread_tip_and_reparent_and_succeed_when_ref_update_422_twice', async () => {
  const gh = fakeGitHub({ refReplies: [[422, { message: 'Update is not a fast forward' }], [422, {}], [200, {}]] });
  const sleeps = [];
  const r = await publishImages(FILES, mkCtx({ fetch: gh.fetch, sleep: async (ms) => sleeps.push(ms) }));
  assert.ok(r.urls, JSON.stringify(r));
  assert.equal(refWrites(gh.calls).length, 3);
  assert.equal(gh.calls.filter((c) => c.method === 'GET' && c.url.endsWith('/git/ref/heads/benchdiff-assets')).length, 3, 're-read each attempt');
  assert.equal(sleeps.length, 2);
  assert.equal(gh.calls.filter((c) => c.url.endsWith('/git/blobs')).length, 1, 'blobs created once');
});

test('publishImages_should_switch_to_update_path_when_create_ref_422_already_exists', async () => {
  let reads = 0;
  const gh = fakeGitHub({ refReplies: [[422, { message: 'Reference already exists' }], [200, {}]] });
  const inner = gh.fetch;
  const fetch = async (url, init) => {
    if ((init?.method || 'GET') === 'GET' && url.endsWith('/git/ref/heads/benchdiff-assets') && ++reads === 1) {
      return { ok: false, status: 404, json: async () => ({}), text: async () => '' };
    }
    return inner(url, init);
  };
  const r = await publishImages(FILES, mkCtx({ fetch }));
  assert.ok(r.urls, JSON.stringify(r));
  assert.deepEqual(refWrites(gh.calls).map((c) => c.method), ['POST', 'PATCH']);
});

test('publishImages_should_return_push_failed_without_throwing_after_five_consecutive_422s', async () => {
  const gh = fakeGitHub({ refReplies: Array.from({ length: 9 }, () => [422, {}]) });
  const r = await publishImages(FILES, mkCtx({ fetch: gh.fetch }));
  assert.deepEqual(r, { error: ERRORS['push-failed'], reason: 'push-failed' });
  assert.equal(refWrites(gh.calls).length, 5);
});

test('publishImages_should_return_push_failed_when_fetch_throws_or_api_errors', async () => {
  const boom = await publishImages(FILES, mkCtx({ fetch: async () => { throw new Error('net down ghs_SECRET_TOKEN'); } }));
  assert.equal(boom.reason, 'push-failed');
  assert.ok(!JSON.stringify(boom).includes('SECRET'));
  const gh = fakeGitHub();
  const bad = await publishImages(FILES, mkCtx({ fetch: async (u, i) => (u.endsWith('/git/blobs') ? { ok: false, status: 500, json: async () => ({}), text: async () => '' } : gh.fetch(u, i)) }));
  assert.equal(bad.reason, 'push-failed');
});

test('publishImages_should_use_hashed_tag_path_and_honour_api_url_and_raw_base_when_configured', async () => {
  const gh = fakeGitHub();
  const r = await publishImages(FILES, mkCtx({ fetch: gh.fetch, tag: 'a/../b', apiUrl: 'https://ghe.example/api/v3', rawBase: 'https://raw.example' }));
  const p = r.urls['delta.svg'];
  assert.ok(p.startsWith('https://raw.example/o/r/'));
  assert.ok(!p.slice('https://raw.example/'.length).includes('..'));
  assert.ok(p.includes(`/t-${sha('a/../b').slice(0, 12)}/`));
  assert.ok(gh.calls.every((c) => c.url.startsWith('https://ghe.example/api/v3/repos/o/r/')));
});

test('publishImages_should_stay_disabled_without_token_or_repository', async () => {
  const r = await publishImages(FILES, { fetch: async () => { throw new Error('no network'); } });
  assert.equal(r.reason, 'disabled');
});

// ---- 4.1.2b: gating and reason mapping (no network before the decision) ----
async function gated(extra, expectedReason) {
  let fetches = 0;
  const r = await publishImages(FILES, mkCtx({ fetch: async () => { fetches += 1; throw new Error('must not be called'); }, ...extra }));
  assert.deepEqual(r, { error: ERRORS[expectedReason], reason: expectedReason });
  assert.equal(fetches, 0, 'zero fetch calls');
  assert.ok(!JSON.stringify(r).includes('SECRET'));
}
const prEvent = (head, base, priv = false) => ({ repository: { private: priv }, pull_request: { head: { repo: head && { full_name: head } }, base: { repo: { full_name: base } } } });

test('publishImages_should_return_private_without_network_when_repository_private', async () => {
  await gated({ event: prEvent('o/r', 'o/r', true) }, 'private');
  await gated({ eventName: 'push', event: { repository: { private: true } } }, 'private');
});

test('publishImages_should_return_fork_without_network_for_pull_request_target_from_a_fork', async () => {
  await gated({ eventName: 'pull_request_target', event: prEvent('x/r', 'o/r') }, 'fork');
  await gated({ eventName: 'pull_request_target', event: prEvent(null, 'o/r') }, 'fork');
});

test('publishImages_should_return_fork_without_network_when_head_repo_differs_from_base', async () => {
  await gated({ event: prEvent('x/r', 'o/r') }, 'fork');
  await gated({ event: prEvent(null, 'o/r') }, 'fork');
});

test('publishImages_should_return_unsupported_host_without_network_when_server_url_not_github_com', async () => {
  await gated({ serverUrl: 'https://ghe.example.com' }, 'unsupported-host');
  await gated({ serverUrl: 'https://github.com.evil.example' }, 'unsupported-host');
});

test('publishImages_should_return_no_event_without_network_when_payload_missing_or_unparseable', async () => {
  await gated({ event: undefined }, 'no-event');
  await gated({ event: '{not json' }, 'no-event');
  await gated({ event: undefined, eventPath: path.join(root, 'does-not-exist.json') }, 'no-event');
});

test('publishImages_should_return_disabled_without_network_when_publish_images_false', async () => {
  await gated({ enabled: false }, 'disabled');
});

test('publishImages_should_read_event_from_path_or_json_string_when_given', async () => {
  const f = path.join(require('node:os').tmpdir(), `bd-event-${process.pid}.json`);
  fs.writeFileSync(f, JSON.stringify({ repository: { private: true } }));
  try {
    await gated({ event: undefined, eventPath: f, eventName: 'push' }, 'private');
  } finally { fs.rmSync(f, { force: true }); }
  await gated({ event: JSON.stringify(prEvent('x/r', 'o/r')) }, 'fork');
});

test('publishImages_should_publish_when_non_pr_event_on_public_repo', async () => {
  for (const eventName of ['push', 'workflow_dispatch', 'release']) {
    const gh = fakeGitHub();
    const r = await publishImages(FILES, mkCtx({ fetch: gh.fetch, eventName, event: { repository: { private: false } }, serverUrl: 'https://github.com' }));
    assert.ok(r.urls, `${eventName}: ${JSON.stringify(r)}`);
  }
});

test('publishImages_should_publish_when_same_repo_pr_on_public_repo', async () => {
  const gh = fakeGitHub();
  assert.ok((await publishImages(FILES, mkCtx({ fetch: gh.fetch }))).urls);
});

test('publishImages_should_return_permission_when_write_responds_403', async () => {
  const mk = (failOn) => {
    const gh = fakeGitHub();
    return (u, i) => (failOn(u, i) ? { ok: false, status: 403, json: async () => ({ message: 'ghs_SECRET_TOKEN' }), text: async () => '' } : gh.fetch(u, i));
  };
  for (const failOn of [(u) => u.endsWith('/git/blobs'), (u, i) => i?.method === 'PATCH']) {
    const r = await publishImages(FILES, mkCtx({ fetch: mk(failOn) }));
    assert.deepEqual(r, { error: ERRORS.permission, reason: 'permission' });
  }
});

// ---- 4.1.2c: robustness and retention ----
const resp = (status, body = {}, headers = {}) => ({
  ok: status < 400,
  status,
  headers: { get: (k) => headers[k.toLowerCase()] ?? null },
  json: async () => body,
  text: async () => JSON.stringify(body),
});
// Fails the first `times` calls matching `match` with `make()`, then delegates to the fake.
function flaky(gh, match, times, make) {
  let left = times;
  return async (u, i) => (match(u, i) && left-- > 0 ? make() : gh.fetch(u, i));
}

test('publishImages_should_retry_with_backoff_when_server_errors_then_succeed', async () => {
  const gh = fakeGitHub();
  const sleeps = [];
  const fetch = flaky(gh, (u) => u.endsWith('/git/blobs'), 2, () => resp(502));
  const r = await publishImages(FILES, mkCtx({ fetch, sleep: async (ms) => sleeps.push(ms) }));
  assert.ok(r.urls, JSON.stringify(r));
  assert.deepEqual(sleeps, [250, 500], 'exponential, jitter 0');
});

test('publishImages_should_add_jitter_and_stop_after_five_attempts_when_5xx_persists', async () => {
  const sleeps = [];
  let calls = 0;
  const r = await publishImages(FILES, mkCtx({ fetch: async () => { calls += 1; return resp(503); }, random: () => 1, sleep: async (ms) => sleeps.push(ms) }));
  assert.equal(r.reason, 'push-failed');
  assert.equal(calls, 5);
  assert.deepEqual(sleeps, [500, 1000, 2000, 4000]);
});

test('publishImages_should_retry_secondary_rate_limit_403_honouring_retry_after_instead_of_permission', async () => {
  const gh = fakeGitHub();
  const sleeps = [];
  const fetch = flaky(gh, (u) => u.endsWith('/git/blobs'), 1, () => resp(403, { message: 'x' }, { 'retry-after': '7' }));
  assert.ok((await publishImages(FILES, mkCtx({ fetch, sleep: async (ms) => sleeps.push(ms) }))).urls);
  assert.deepEqual(sleeps, [7000]);
  const byBody = flaky(fakeGitHub(), (u) => u.endsWith('/git/blobs'), 1, () => resp(403, { message: 'You have exceeded a secondary rate limit.' }));
  assert.ok((await publishImages(FILES, mkCtx({ fetch: byBody }))).urls);
  const byRemaining = flaky(fakeGitHub(), (u) => u.endsWith('/git/blobs'), 1, () => resp(403, {}, { 'x-ratelimit-remaining': '0' }));
  assert.ok((await publishImages(FILES, mkCtx({ fetch: byRemaining }))).urls);
});

test('publishImages_should_return_push_failed_not_permission_when_rate_limit_403_persists', async () => {
  const r = await publishImages(FILES, mkCtx({ fetch: async () => resp(403, {}, { 'retry-after': '1' }) }));
  assert.equal(r.reason, 'push-failed');
});

test('publishImages_should_not_commit_and_reuse_tip_urls_when_tree_unchanged', async () => {
  const gh = fakeGitHub();
  const fetch = async (u, i) => (i?.method === 'POST' && u.endsWith('/git/trees') ? (gh.fetch(u, i), resp(201, { sha: TIP_TREE })) : gh.fetch(u, i));
  const r = await publishImages(FILES, mkCtx({ fetch }));
  const p = `t-${sha('default').slice(0, 12)}/${sha(SVG).slice(0, 8)}/delta.svg`;
  assert.deepEqual(r, { urls: { 'delta.svg': `https://raw.githubusercontent.com/o/r/${TIP}/${p}` } });
  assert.equal(gh.calls.filter((c) => c.url.endsWith('/git/commits') && c.method === 'POST').length, 0);
  assert.equal(refWrites(gh.calls).length, 0);
});

test('publishImages_should_never_delete_or_force_and_keep_previous_tip_as_parent_when_publishing', async () => {
  const gh = fakeGitHub({ refReplies: [[422, {}], [200, {}]] });
  await publishImages(FILES, mkCtx({ fetch: gh.fetch }));
  assert.ok(gh.calls.every((c) => c.method !== 'DELETE'));
  for (const w of refWrites(gh.calls)) assert.equal(w.body.force ?? false, false);
  for (const c of gh.calls.filter((x) => x.method === 'POST' && x.url.endsWith('/git/commits'))) assert.deepEqual(c.body.parents, [TIP]);
  for (const c of gh.calls.filter((x) => x.url.endsWith('/git/trees'))) assert.equal(c.body.base_tree, TIP_TREE);
});

test('publishImages_should_keep_hostile_tags_out_of_paths_and_use_github_raw_host_for_github_com', async () => {
  for (const tag of ['..', 'a/../b', '/abs', 'x:y']) {
    const gh = fakeGitHub();
    const { rawBase, ...rest } = mkCtx({ fetch: gh.fetch, tag, serverUrl: 'https://github.com' });
    const r = await publishImages(FILES, rest);
    const rel = r.urls['delta.svg'].slice('https://raw.githubusercontent.com/o/r/'.length).split('/').slice(1).join('/');
    assert.equal(rel, `t-${sha(tag).slice(0, 12)}/${sha(SVG).slice(0, 8)}/delta.svg`);
  }
});

test('rewritePlaceholders_should_warn_once_and_strip_picture_and_image_when_publish_fails', () => {
  const pic = '<picture><source media="(prefers-color-scheme: dark)" srcset="benchdiff-svg:a-dark.svg"><img alt="x" src="benchdiff-svg:a-light.svg"></picture>';
  const md = `${MARKER}\n\n${pic}\n\n![b](benchdiff-svg:b.svg)\n\n${DETAILS}\n`;
  const r = rewritePlaceholders(md, { error: ERRORS.fork, reason: 'fork' });
  assert.ok(!r.markdown.includes(PLACEHOLDER_SCHEME));
  assert.ok(!/<picture|<source|<img|!\[b\]/.test(r.markdown));
  assert.equal(r.markdown.split('charts could not be hosted').length - 1, 1);
  assert.ok(!/\n{3,}/.test(r.markdown));
});

test('rewritePlaceholders_should_strip_whole_picture_when_only_one_of_its_urls_is_hosted', () => {
  const pic = '<picture><source srcset="benchdiff-svg:a-dark.svg"><img alt="x" src="benchdiff-svg:a-light.svg"></picture>';
  const r = rewritePlaceholders(`${MARKER}\n\n${pic}\n`, { urls: { 'a-light.svg': 'https://h/l.svg' } });
  assert.ok(!/<picture|<source|<img|https:\/\/h\/l/.test(r.markdown));
  assert.ok(r.warning);
});
