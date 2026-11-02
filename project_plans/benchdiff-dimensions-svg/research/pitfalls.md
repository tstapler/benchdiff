# Pitfalls research: benchdiff-dimensions-svg

Date: 2026-10-31. Labels: VERIFIED = I ran the command or opened the primary page this session; REPORTED = community/secondary source, not independently confirmed; UNVERIFIED = no source found.

## 1. Recommendation (ranked)

Delivery must be a hosted image URL referenced by `![alt](url)` or `<picture>`. Ranking for "SVG in sticky comment + job summary":

| Rank | Mechanism | Public repo | Private repo | Fork PR (read-only token) | Verdict |
|---|---|---|---|---|---|
| 1 | Orphan/dedicated branch (e.g. `benchdiff-assets`) served via `https://raw.githubusercontent.com/<o>/<r>/<sha>/<path>.svg`, content-addressed path | Works, `image/svg+xml` | Broken for anonymous/proxy fetch (404); see 2.3 | Cannot push (read-only token) -> fallback | Best default for public repos |
| 2 | Release asset (`browser_download_url`) | Works only if MIME is right (see 2.5); REPORTED `application/octet-stream` | Needs auth | Cannot create | Not recommended for SVG; keep for the release workflow as a *file* attachment |
| 3 | Workflow artifact with `archive: false` (upload-artifact v7) | URL needs login | Needs login | Upload works, but link needs login | Link-only, not inline image; use as the always-available fallback download |
| 4 | `user-attachments` upload (what the web UI uses) | n/a | n/a | n/a | Reject: no public API, needs browser session |
| 5 | `data:` URI in `<img>` | Stripped | Stripped | Stripped | Reject |
| 6 | Inline `<svg>` | Stripped | Stripped | Stripped | Reject (already known) |
| 7 | External renderer (e.g. jsDelivr `cdn.jsdelivr.net/gh/...`) | Works, `image/svg+xml`, long cache | Public repos only | Cannot push | Optional alternative URL base; adds third-party dependency |

**Recommended policy**: (a) Always write `.svg` files and upload them as an unzipped artifact (works everywhere, incl. forks, as a download link). (b) When the repo is public AND the token can write contents (`contents: write`, non-fork), commit SVGs to an orphan branch under a content-hash path and embed the commit-SHA-pinned raw URL. (c) Otherwise (private repo, fork PR, push failure) fall back to Unicode/Markdown tables with an explicit warning line naming why SVGs were skipped. Opt-in only.

Open design point for the ADR: private repos have no verified anonymous-viewable image path with only `GITHUB_TOKEN`; treat "private repo = fallback to artifact link + Markdown" as the v1 behavior unless the owner tests the `github.com/<o>/<r>/raw/...` form (REPORTED below, UNVERIFIED for comments).

## 2. GitHub rendering facts

### 2.1 Sanitization
- GitHub's open-source sanitizer allowlist (html-pipeline) has no `svg` element; `img` allows only `src, longdesc, loading, alt`, with `src` protocols `http`, `https`, relative (so `data:` is out); `picture` and `source[srcset]` are allowed. VERIFIED, source: https://github.com/jch/html-pipeline/blob/main/lib/html_pipeline/sanitization_filter.rb. Caveat: this is the OSS gem, not necessarily github.com's exact production config (INFERRED that they match).
- `data:` URIs: github/markup#270 (2014-2018) staff cited an SVG-with-script security concern; later comments say it "neither working nor implemented". REPORTED/dated: https://github.com/github/markup/issues/270. No current official statement found; treat as unsupported.
- Step summary: docs say summaries support GitHub-flavored Markdown, secrets are auto-masked, 1 MiB per step, max 20 summaries displayed per job, over-limit upload fails with an error annotation but does not fail the step. VERIFIED: https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-commands. The docs do not state HTML/image sanitization rules; summary images use the same Markdown pipeline (INFERRED, must be verified in the real PR run per requirements).

### 2.2 Camo
- GitHub proxies images via Camo, HMAC-signed anonymized URLs; anyone with the URL can view; images behind auth or on private networks cannot be fetched by GitHub; Camo caches per origin `Cache-Control`/`Expires`; mismatched `Content-Type` vs Camo's whitelist breaks images; purge via `curl -X PURGE <camo url>`. VERIFIED: https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/about-anonymized-urls. The page does not mention SVG.
- Upstream Camo: content-type whitelist (list not shown), default max 5 MiB via `Content-Length`. VERIFIED for the OSS project only: https://github.com/atmos/camo. Whether github.com's whitelist includes `image/svg+xml`: UNVERIFIED directly, but SVGs from raw URLs routinely render in READMEs (INFERRED); the requirements' own verification step (view in real comment) must confirm.
- Consequence for private repos: Camo fetches anonymously, so private-repo raw URLs cannot render for anyone (INFERRED from the above two points; REPORTED symptom in https://github.com/orgs/community/discussions/23385 which found no working tokenized-URL answer).

### 2.3 raw.githubusercontent.com
Ran `curl -I` on a public SVG (gilbarbara/logos `logos/100tb.svg`, 2026-11-01):
- `content-type: image/svg+xml`, `x-content-type-options: nosniff`, `content-security-policy: default-src 'none'; style-src 'unsafe-inline'; sandbox`, `cache-control: max-age=300`. VERIFIED (own command). So raw does NOT serve valid SVG as `text/plain`; text/plain appears on 404 responses. (My earlier hypothesis was wrong.)
- The CSP means opening the raw URL directly cannot run script; `style-src 'unsafe-inline'` is why `<style>` inside SVG still styles when loaded as an image. Note `<img>` mode disables script anyway: https://www.w3.org/TR/SVG/conform.html (secure animated/static mode), REPORTED via search; Mozilla blocks external loads in SVG-as-image: https://bugzilla.mozilla.org/show_bug.cgi?id=628747.
- `github.com/<o>/<r>/raw/<ref>/<path>` returns 302 to raw.githubusercontent.com (VERIFIED, curl). For private repos the redirect carries a short-lived token only for a logged-in browser (REPORTED: https://github.com/orgs/community/discussions/23385), which Camo cannot use.
- Caching: 300 s default, community reports of longer (86400) and stale reads: https://github.com/orgs/community/discussions/46691, https://github.com/orgs/community/discussions/46758 (REPORTED). Mitigation: never overwrite; use a commit-SHA or content-hash path so each URL is immutable, which also sidesteps Camo caching.
- jsDelivr for contrast (VERIFIED curl): `image/svg+xml`, `cache-control: public, max-age=604800, s-maxage=43200`. Branch refs would be stale for up to 12h, so only use SHA-pinned URLs.

### 2.4 Orphan branch / gh-pages
- Needs `contents: write`; on `pull_request` from forks `GITHUB_TOKEN` is read-only, VERIFIED: https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows ("The `GITHUB_TOKEN` has read-only permissions in pull requests from forked repositories"). Repo README already documents this limitation for the comment (README.md line 36-37), so the existing fork fallback pattern (warn, report goes to summary) extends naturally.
- `pull_request_target`/`workflow_run` could write but are a known unsafe pattern with untrusted PR content: https://docs.github.com/en/actions/reference/security/secure-use. Do not recommend; SVGs are generated from fork-controlled benchmark names, which makes this worse.
- Branch growth: one SVG per PR run accrues in history; need a documented prune or a single-commit force-push strategy (design note, no source).
- Concurrency: parallel runs pushing to one branch need retry/rebase (INFERRED).
- GitHub Pages is not needed and would add a Pages-enable permission; skip.

### 2.5 Release assets
- Release downloads are REPORTED to be served as `application/octet-stream`; browsers will not paint SVG with that type and Camo's whitelist would reject it: https://devactivity.com/insights/github-release-downloads-the-missing-mime-type-mystery-and-workarounds-for-developers (secondary, low-quality site) and https://github.com/lowlighter/metrics/discussions/1548 (title only matched). I did not test an SVG release asset: UNVERIFIED. Also needs a release to exist, so it does not suit PR comments.

### 2.6 Workflow artifacts
- `actions/upload-artifact` v7 `archive: false` uploads one unzipped file; `artifact-url` output "Users must be logged-in in order for this URL to work". VERIFIED: https://github.com/actions/upload-artifact. Changelog: browsers can display some file types inline when downloaded unzipped, but this is a download flow, not an anonymous image URL. VERIFIED title/summary: https://github.blog/changelog/2026-02-26-github-actions-now-supports-uploading-and-downloading-non-zipped-artifacts/.
- Artifact URLs cannot be fetched by Camo (needs login), so `![](artifact-url)` will break in comments for the same reason as private repos. A gh-aw PR claims artifact URLs can feed `![Chart](URL)` but the page did not state auth behavior: UNVERIFIED, https://github.com/github/gh-aw/pull/25923. Test before trusting.
- Works on fork PRs (upload needs no write token) - INFERRED.

### 2.7 user-attachments
No public API; community tools replay internal browser-session endpoints, which break and need cookies/CSRF: https://pkg.go.dev/github.com/drogers0/gh-image@v1.1.0 (REPORTED). Reject.

### 2.8 Dark mode
- `<picture>` with `<source media="(prefers-color-scheme: dark)" srcset=...>` plus fallback `<img>` is documented and allowlisted (`picture`, `source[srcset]`; also `media` is an allowed global attr). VERIFIED: https://github.blog/changelog/2022-05-19-specify-theme-context-for-images-in-markdown-beta/ and the sanitizer list above. The doc page at docs.github.com basic-writing syntax confirms `<picture>` is supported.
- `#gh-dark-mode-only` / `#gh-light-mode-only` fragments: REPORTED as deprecated in favor of `<picture>` by some, still working per a 2026 post; no official statement found: https://tenthirtyam.pages.dev/dispatches/2026/04/05/light-and-dark-mode-images-in-github-markdown/. UNVERIFIED -> use `<picture>`.
- Cheapest robust option: ONE SVG with `<style>@media (prefers-color-scheme: dark){...}</style>` and CSS variables. Caveat: inside `<img>`, the media query follows the *OS/browser* preference, not the GitHub theme setting, so a user with GitHub dark theme on a light OS sees light colors (INFERRED from CSS spec behavior; verify visually). To be safe on both, use mid-tone colors with sufficient contrast on both white and #0d1117, plus explicit background rect, or emit two files and use `<picture>` (doubles hosting).
- `<picture>` + two SVGs means two hosted files and a larger comment; account for it in the 65 536 budget.

## 3. Size limits
- Comment body max 65 536 characters: REPORTED by tool maintainers (error text "maximum is 65536 characters"): https://www.github.com/mshick/add-pr-comment/issues/93, https://github.com/github/issue-metrics/issues/258. No GitHub doc page opened. Unclear whether characters or UTF-8 bytes (one commit comment says bytes): UNVERIFIED -> stay with bytes-conservative budget. The repo already enforces `COMMENT_LIMIT = 60000` (index.js:12) and truncation with an existing marker (index.js:131). New image markup is small (URL + alt) but alt text built from benchmark names must count toward the budget and truncation must not cut inside a `<picture>`/`![]()` element (extend the existing "closers" logic).
- Job summary 1 MiB per step (VERIFIED above), so SVGs must never be inlined there anyway; only references.
- Camo upstream 5 MiB cap (OSS default). Keep each SVG far smaller, target < 100 KB; a 10 impl x hundreds benchmark chart must cap rows (already in requirements).
- Artifact/branch storage: no new limit found.

## 4. SVG security and robustness
- **XML injection from benchmark names** (and labels, units, group names, `title`): escape `& < > " '` in text and attribute values; also strip/replace characters invalid in XML 1.0 (control chars other than tab/LF/CR, U+FFFE/FFFF, lone surrogates cannot occur in Rust `str` but U+0000-U+001F can). An unescaped `]]>` or `-->` matters inside CDATA/comments; avoid emitting CDATA and comments from user text. The `<title>`/`<desc>` content needs the same escaping. Source: XML 1.0 spec char production, https://www.w3.org/TR/xml/#charsets (standard, not opened this session; INFERRED from spec knowledge).
- **Never emit** `<script>`, `<foreignObject>`, `<image href>`, `<a href>`, `xlink:href`, `on*` attributes, or external `url()` in CSS from any input. The generator should be a closed template: only numbers and escaped text nodes are interpolated; never user text in attribute names/ids/`style`. Add a test that a name like `"><script>alert(1)</script>` and `]]>`/`&#x0;` round-trips as inert text and the document parses with an XML parser.
- Even if escaping failed, `<img>`-embedded SVG is script-disabled (see 2.3), but standalone `.svg` opened directly (README links, artifacts) is not. Standalone files should be treated as the risky path. Raw serves them with CSP `sandbox`, artifact download does not (UNVERIFIED).
- **Numeric hygiene**: existing finite-number rule extends; NaN/inf in coordinates produce invalid SVG. Format floats with a fixed precision function (not `{}`), clamp, and never print `-0`.
- **Text width without fonts**: no measurement in WASM. Use a per-character width table (e.g. ~0.6em avg monospace-ish for ASCII, 1.0em for CJK/wide, 0.3em for narrow chars `iljt.,:`), measure with a conservative over-estimate, then truncate with an ellipsis and put the full name in `<title>`. Declare `font-family: system-ui, sans-serif` with a monospace stack for names for predictability; set `textLength`/`lengthAdjust` only as a last resort since it distorts. Keep columns sized from the *estimated* max width plus slack, and cap the label column. No source citations; design guidance, UNVERIFIED against rendered output - test in Chrome, Firefox, Safari and GitHub mobile.
- **Deterministic output**: no HashMap iteration (use BTreeMap/sorted Vec), no timestamps/random ids, fixed float formatting, stable id derivation (counter or hash of content), `\n` newlines, no locale. Also ensure content-hash paths hash the final bytes. WASM vs native f64 formatting should be identical but add a test comparing native and WASM output (the golden shim in test/golden.test.js already runs WASM).
- Accessibility: `role="img"`, `<title>` + `<desc>`, `aria-labelledby`; alt text in Markdown must be short (full numbers go in the table, which stays as the primary content).

## 5. Statistics: multiple comparisons
- N-way vs reference gives (N-1) pairs per benchmark times M benchmarks; per-test alpha inflates false positives. Holm step-down controls family-wise error with no independence assumption and is uniformly more powerful than Bonferroni; Benjamini-Hochberg controls FDR instead. Secondary sources only: https://metricgate.com/blogs/holm-vs-bonferroni-multiple-testing/, https://metricgate.com/blogs/bonferroni-holm-hochberg/ ; primary is Holm 1979, Scand. J. Statist. 6:65-70 (not retrieved; UNVERIFIED by me).
- Existing precedence is Mann-Whitney / interval / threshold. Holm only applies to p-value-based tests; the interval and threshold rules have no p-value. Risk: applying a correction only to one of three rules gives inconsistent verdicts. Recommendation: compare-to-reference only (no all-pairs ranking significance), apply Holm per benchmark across the (N-1) pairs *only when Mann-Whitney is the deciding rule*, make it opt-out via config, and always report which rule decided. Keep default two-way path uncorrected (m=1 makes Holm a no-op, so byte-identity holds).
- Ranking charts imply ordering; do not draw rank bars as "significant" unless the pair test says so; show ties.

## 6. Name parsing ambiguity
- Conventions collide: Go `BenchmarkSort/impl=quick-8` (the trailing `-8` is GOMAXPROCS), Go sub-benchmark `size=1000`, JMH `Class.method:param` and `(size = 1000)` params, Criterion `group/function/param`, Google Benchmark `name/arg:1/real_time`. A single heuristic will mis-split (e.g. `/` inside a param value, `-8` parsed as a param, `=` absent). Requirements already say: explicit rule, not guessing. Recommend default = no dimension inference except per-importer well-known suffixes, and user rule via template/regex with named captures, plus a diagnostic listing unparsed names. Include a `strict` mode that errors when a name does not match. (Design guidance; I did not survey each tool's docs this session: UNVERIFIED per-format claims above are from knowledge.)
- Identity collisions: two different raw names can map to the same dimension tuple after extraction (e.g. different suffixes stripped). Must detect duplicates and fail loudly/skip with a report, mirroring the existing "skipped metrics" surfacing.
- Missing cells (impl absent for some benchmark) must render as blank/`n/a`, not 0, and not shift columns.
- Label-from-file (`label=path`) conflicts with name-derived implementation: define precedence and error on conflict.

## 7. WASM and bundle growth
- Current `dist/benchdiff.wasm` = 330 650 bytes (VERIFIED `ls -l`); release profile already `opt-level="s"`, `lto`, `strip`, `panic=abort` (Cargo.toml lines 16-20). Risks: a regex crate (large, ~hundreds of KB), a TOML/YAML parser, and float-formatting machinery. Mitigations: hand-written glob/template matcher instead of `regex`; JSON config only (serde_json already present) or a tiny TOML subset; avoid `format!` with `{:?}` on floats in hot paths; run `wasm-opt -Oz` if available (UNVERIFIED present in the toolchain). Set a size budget in CI (e.g. fail if wasm grows > X%) so growth is a conscious decision.
- Config file in WASI: the sandbox needs a preopened directory for the config path; the core is I/O-free, so read the file in `index.js` (node) / CLI and pass contents in. This also keeps precedence logic testable.
- The zero-dependency `index.js` has to gain: git push to orphan branch (via `git` CLI or Git Data REST API with `fetch`), artifact upload (the artifact service API is not trivially callable without `@actions/artifact`; since the action is zero-dependency, easiest is to document using `actions/upload-artifact` as a separate workflow step and have benchdiff only write the files and output paths. INFERRED; verify).
- Branch push via REST: `PUT /repos/{o}/{r}/contents/{path}` needs one call per file; Git Data API (blob/tree/commit/ref) lets one commit hold all SVGs. Both need `contents: write`.

## 8. Backward-compat traps
- Golden byte-identity: `test/golden.test.js` + `testdata/golden/` (base.go.txt/head.go.txt only today, so coverage of rendering variants is thin). Add goldens for the default path with new options *unset* before touching the renderer, and guard against incidental changes: new struct fields serialized into JSON must use `skip_serializing_if` (as `model.rs` already does for some fields) so schema-1 JSON bytes do not change; do not reorder maps; do not change the sticky-comment marker, truncation text, or heading layout.
- Schema: `MetricSet::parse` ignores unknown fields and rejects `schema > SCHEMA_VERSION` (model.rs:150-170), comment says "v1 only gains optional fields". So an additive optional `dims` map on a metric stays schema 1 and is read fine by new and old readers (old readers ignore it). A bump to 2 makes every older benchdiff (e.g. the pinned `@v0` action reading a baseline JSON from the cache/artifact of a newer run) fail with the "newer than supported" error: a one-way trap for cross-version baselines. Recommend: additive `dims` only; emit `dims` only when dimensions are enabled; N-way file labels live in `env` or a new optional top-level key.
- Caveat: if identity ever becomes `(group,name,measure,dims)`, old readers will merge or mismatch metrics that differ only by `dims`. Keep `name` unchanged as the full raw name and treat `dims` as derived annotation, so schema-1 identity still holds.
- Action outputs (`report-path`, `json-path`) and inputs unchanged; add new outputs only. `@v0` floating tag moves only after real-PR verification (already in Risk Control).
- Sticky comment: a comment that now contains images must still be found by the existing marker; truncation must not strip the marker (existing code appends it after truncation, index.js:131).
- Opt-in default off preserves behavior when the SVG branch push or permissions fail; failure to host must be non-fatal and surface a warning line, never fail the job (matches the existing fork-PR warning behavior in README).

## 9. Must-test-in-a-real-PR checklist (cannot be settled from docs)
1. SVG from raw SHA-pinned URL renders in a PR comment and in `$GITHUB_STEP_SUMMARY` (public repo), light and dark GitHub theme, desktop and mobile.
2. Same in a private repo (expected to fail; confirm and check the `github.com/.../raw/...` variant).
3. Artifact `archive:false` URL as `![](url)`: expected broken for Camo; confirm.
4. Release asset SVG content-type.
5. Single-SVG `prefers-color-scheme` behavior vs GitHub theme setting.
6. Camo handling of `image/svg+xml` and any size cap.
7. Fork PR: no write, warning path, summary still correct.
8. Comment character vs byte limit with non-ASCII benchmark names.
