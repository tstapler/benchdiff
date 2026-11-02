# Requirements: benchdiff-dimensions-svg

**Date**: 2026-10-31
**Type**: feature addition
**Complexity**: 3 — system design (several epics: data model, N-way compare, SVG renderer, config, Action delivery of images)

## Problem Statement
benchdiff only compares two runs (base vs head) and renders Markdown tables. Users cannot (a) compare several implementations of the same interface side by side, (b) slice results across more than the fixed `(group, name, measure)` identity, (c) get a visual (SVG) summary they can embed in PR comments, job summaries, READMEs and release notes, or (d) tune the report beyond a handful of Action inputs.

## Baseline
Today: two-way base/head only; metric identity is `(group, name, measure)`; output is Markdown + JSON; configuration is the ~12 flat Action inputs / CLI flags in `README.md`; the only "visual" is the `▰▰▰▱▱` bar in the Change column (`crates/benchdiff-core/src/markdown.rs`). Comparing implementations means running benchdiff once per pair and reading separate tables, or hand-rolling it in benchstat. This is the regression anchor for Phase 6: every existing golden case (`test/golden.test.js`, `testdata/golden/*`) must keep producing byte-identical Markdown/JSON unless a new option is enabled.

## Primary problem (falsifiable)
A library maintainer who reviews a pull request touching one of several implementations of the same interface cannot tell, from the PR comment alone, which implementation is fastest on each benchmark and whether the gap is statistically significant.

What failure looks like today (per the Baseline section above; stated there from the code and README, not re-run for this review): with K = 3 implementations the reviewer gets K-1 = 2 separate pairwise comments or must run benchdiff K-1 times and collate the tables by hand; no table ranks the implementations against each other, no geometric mean spans them, and the only visual is the `▰▰▰▱▱` bar.

How this claim can be wrong: it is falsified if the baseline run of the walkthrough in "Outcome metric" (the same three questions asked of today's output on the `testdata/nway` data, split into pairwise runs) is answered correctly from that output in under 60 seconds by at least 2 of 3 readers, the same threshold the outcome metric sets for the treatment. The problem is then smaller than assumed and the scope should shrink to the dimensions and config work.

The baseline half runs early, not at the end: plan task 0.3.1a (first wave) commits the `testdata/nway` inputs and today's pairwise output, and gate G0 (task 0.3.1b) records the readers' result before the matrix and chart work (wave 6) starts. The treatment half is MAN-7 in plan task 5.1.3a. If the baseline already meets the target, the plan stops after the dimensions and config phase.

## Primary persona and job to be done
Primary: **the library maintainer** who owns an interface with several implementations (sort algorithms, parser backends, storage engines) and reviews PRs that change one of them. Job to be done: *when I open or review a PR that changes an implementation, I want the PR page to show how every implementation compares to the baseline one and whether the difference is real, so that I can approve or push back without downloading artifacts.* Secondary: the release manager who attaches standalone SVGs to release notes (existing reusable workflow), and the CLI user running the same comparison locally. The two-way base/head user is unchanged and not the target of this feature.

## Demand evidence (what is known and what is not)
- Known, from this repository: `docs/research.md` section 1 (a survey of three of the owner's repos, read from their workflows and build files) records that tymux benchmarks two backends against each other (`tymux vs tmux`) and renders its own SVG charts, that stapler-squad runs `benchstat` plus a hand-written PR comment script, and that the three repos have three bespoke diff-and-comment scripts. `README.md` ("Baseline in a PR workflow") names stapler-squad as the origin of the cache-the-baseline pattern. These show the owner's own repos have the need for a pairwise comment and, in tymux's case, for multi-implementation charts.
- Not established: that stapler-squad or any other repository has adopted benchdiff (not verified here); that anyone outside the owner has asked for N-way or charts (no issue or request was reviewed for this plan, and benchdiff collects no usage data by design); how many maintainers have the multi-implementation shape. The plan therefore treats demand beyond the owner's repos as INFERRED from the general prevalence of the pattern, not measured.

### Adoption proxy (no telemetry)
benchdiff collects no usage data by design, so adoption after release is counted by hand from public information (plan requirement OM-2, manual check MAN-8 in `implementation/validation.md`). At release+30 days and again at release+90 days Tyler records: (a) how many of the owner's repos (the three surveyed in `docs/research.md` section 1, plus any added since) have a workflow that sets `variants` or `charts` for this Action, by grepping their workflow files; (b) how many public repositories outside the owner's do, via `gh search code 'tstapler/benchdiff' --filename '*.yml'` filtered for those inputs (INFERRED to under-count: private repositories and code-search lag are invisible). Proposed target, not a measurement: at least 2 of the owner's repos at +30 days. Reading of the result: zero at +90 days means demand beyond the owner is unproven, so further hosting-related investment waits for an external request; it never triggers a rollback, because every feature is opt-in.

## Outcome metric
Time-to-signal on the fixture comment. A reader who has not seen the tool is shown only the sticky comment for `testdata/nway` (three variants, six benchmarks) and, without opening any artifact, answers: (Q1) which variant has the best overall result (geomean) and against which reference, (Q2) whether the gap on one named benchmark is significant, (Q3) whether any variant is slower than the reference. Target (a proposal to be confirmed, not a measured baseline): all three correct in under 60 seconds for at least 2 of 3 readers (Tyler plus two others, or a recorded waiver). The same three questions are first asked of today's output run pairwise (the baseline). Verification is the manual walkthrough MAN-7 and the ratio-vs-percent reading check MAN-6 in `implementation/validation.md`, run in plan task 5.1.3a. This is a post-release product check; it does not gate the automated tests.

## Roadmap fit (ICE, INFERRED)
No roadmap file exists in the repository (top-level listing checked), so this item cannot be ranked against other roadmap entries; it is compared with the follow-ups already named in this folder (importer-level dimensions, TOML config, BH/FDR across rows), each of which depends on the dimensions and matrix work here. Judgment scores on a 1-10 scale, all INFERRED, none measured: Impact 7 for the primary persona and 2 for everyone else; Confidence 4 (the need is real in the owner's repos, external demand is unknown); Ease 3 (Large band, about 12.7M CU in the plan's estimate, plus three human gates). ICE (mean) 4.7. The score says "worth doing for the owner's repos, ship in stages and watch for demand", not "high-confidence broad demand".

## Risky assumption and how the plan de-risks it
Assumption: users want N-way comparison *and* SVG charts together, and want the charts embedded in the PR comment. The riskiest half is the SVG half, because GitHub strips inline `<svg>` and every hosting mechanism has a trade-off (hosting rabbit hole above). De-risking built into the plan: (1) the N-way table, `variants` input and CLI are complete and useful without any image (waves W10-W14 of the plan), so the N-way value never waits on SVG hosting; (2) spike gate G1 (task 0.2.1, a human check on a real PR, public, private and fork) happens before any publisher code, and a failed spike leaves the text chart plus standalone SVG files as the floor (ADR-003 ladder); (3) charts are opt-in (`charts` defaults to `none`); (4) the support matrix restates metric 3 per repository class instead of promising one outcome. Stop signal: if G1 shows no inline rung works for public same-repo PRs, the hosting epic (4.1.2) is not built and the SVG work ships as files only.

## Users / Consumers
- Maintainers using the GitHub Action in PR workflows and the reusable release-benchmarks workflow.
- CLI users running `benchdiff` locally.
- Downstream readers of PR comments, job summaries, READMEs and release notes.
- Consumers of the canonical JSON (schema 1) and the `report-path`/`json-path` Action outputs.

## Success Metrics
- Given N ≥ 3 implementations of one benchmark (from sub-benchmark names/params **or** explicitly labelled files), one report shows all N side by side with a chosen reference (the first input unless `reference` names another; the fastest variant is only a per-row ranking marker, never the reference, ADR-005), instead of N−1 separate runs. Verified on a fixture with ≥ 3 implementations.
- A user can pick any dimension present in the data (implementation, a param like `size=1000`, or group) as the comparison axis and pivot on it without editing benchmark code (`measure` is not an axis; see the scope decisions).
- SVG output renders correctly in all three targets: PR sticky comment, job summary, standalone file. Verified by actually viewing each rendering (comment/summary via a hosted image URL, since GitHub strips inline `<svg>`). Restated in the plan phase as a per-repo-class support matrix (public same-repo, no `contents: write`, fork PR, private, GHES, non-PR event): the inline hosted image is the public same-repo expectation; every other class gets an in-body notice plus the chart's rows as an opened text block, and every class gets the standalone files. Verified class by class in plan task 5.1.3a, with explicit waivers where a class cannot be run.
- Outcome metric (see "Outcome metric"): a reader answers best-geomean-and-against-which-reference, is-it-significant and anything-slower-than-reference from the N-way comment alone, in under 60 seconds, without opening an artifact.
- With no new options set, output is byte-identical to today (golden tests pass untouched).
- SVGs are deterministic (same input → same bytes), readable in light and dark themes, and carry text alternatives (`<title>`/`<desc>`, alt text in Markdown).

## Size
Large (3–15M CU) — informational estimate, bands INFERRED (see ESTIMATION.md). Full scope is the default.

**Wall-clock blockers**: owner decisions on config-file format and SVG hosting mechanism (resolved in plan/ADR); a real GitHub PR run on this repo to verify image rendering in comment and job summary (needs CI run + token permissions); rebuild and commit of `dist/benchdiff.wasm`.

## Constraints
- Core stays I/O-free Rust compiled to WASM; the Action stays a zero-dependency `index.js` on `node24` with no run-time downloads or toolchain.
- SVG generation must work inside the WASM/WASI sandbox (no system fonts, no filesystem beyond what the CLI already uses): text metrics must be estimated, not measured.
- GitHub strips `<svg>` from comments and step summaries, so delivery needs a hosted image reference (mechanism to be chosen in research/ADR; must not require secrets beyond `GITHUB_TOKEN`).
- Backward compatible: existing inputs/flags, canonical JSON schema 1 readers, Action outputs, and sticky-comment marker keep working. Schema changes must be additive (readers reject newer schema, so a bump needs a migration story).
- Untrusted-input hygiene already enforced (title is trusted Markdown, `tag` charset, finite numbers) extends to anything rendered into SVG: all names/labels must be XML-escaped.

## Non-functional Requirements
- **Performance SLO**: report generation stays interactive (< 1 s for 10k metrics, native or WASM); SVG size bounded (cap rows/series, with an explicit "N more not shown" indicator).
- **Scalability**: up to ~10 implementations × hundreds of benchmarks; larger inputs degrade by capping (matrix columns and chart series are capped at 8 with a visible notice and a JSON pointer), never by failing silently.
- **Security classification**: public.
- **Data residency**: no special requirements; no data leaves the runner except via the user's own PR/release.

## Scope
### In Scope
- **Dimensions**: generalize a metric's identity to named dimensions (implementation/variant, params parsed from names e.g. `/impl=quick`, `[size=1000]`, group, measure); choose which dimension is the comparison axis and which is the row key. Sources: derived from sub-benchmark names/params **and** explicit labels on input files (`label=path` style).
- **N-way comparison**: N implementations vs a chosen reference (default: the first input unless `reference` names another; "fastest" differs per row, so it is only a ranking marker, not the reference: ADR-005), per-benchmark ranking, relative-to-reference deltas, significance per pair using existing Mann-Whitney / interval / threshold precedence, and a Markdown matrix. Base/head two-way remains the default and is a special case.
- **SVG diagrams** (core renders, deterministic): at least a delta bar/dumbbell chart for base-vs-head and a grouped bar/ranking chart for N-way; optional distribution view when samples exist. Light/dark safe colors, accessible text, bounded size.
- **Delivery**: write `.svg` files (CLI `--svg-dir`/flag, Action output paths), embed in Markdown via a hosted image URL for sticky comment and job summary, and ship standalone SVG/HTML artifacts for READMEs/release notes. Fallback when hosting is unavailable: link-free Markdown/Unicode rendering as today, with a warning.
- **Configuration**: a config file (checked into the repo, e.g. `benchdiff.toml`/`.json`) plus existing Action inputs/CLI flags; precedence flags > inputs > file > defaults. Covers: dimensions, reference selection, thresholds per group and per metric, chart selection and sizes, sort, row caps, input formats (`format`, `base-format`); the output file paths (`--markdown`, `--json`) are CLI/Action outputs and deliberately not config keys (scope cut, see decisions).
- Docs/README updates, tests (unit, golden, SVG snapshot), rebuilt committed WASM.

### Out of Scope
- Hosting a service or storing history (no dashboards, no time-series trend store; that is github-action-benchmark/Bencher territory).
- Running benchmarks, or changing the existing importers' parsing beyond exposing dimensions.
- Interactive/JS charts, PNG rasterization, or any run-time dependency download.
- Changing default behavior or output of current inputs when new options are unset.

## Rabbit Holes
- **Hosting SVG for PR comments/job summaries**: no first-class API for uploading a comment image; candidate mechanisms (release asset, workflow artifact, orphan branch / gh-pages, `raw.githubusercontent.com` with caching and private-repo auth, base64 data URI limits) each have trade-offs. Needs a researched decision (ADR), including private repos and fork PRs (read-only token).
- **Text width without fonts** in SVG: label truncation/wrapping heuristics; wrong estimates clip text.
- **Parsing dimensions from names**: ambiguous conventions across Go, JMH, Criterion, Google Benchmark; need an explicit, user-overridable rule (regex/template) rather than guessing.
- **Statistics for N-way**: multiple comparisons inflate false positives; decide whether to correct (e.g. Holm) or only compare to the reference.
- **Config precedence and WASM**: the sandbox's filesystem access for reading a config file; the schema's evolution.
- **Canonical JSON schema**: adding dimensions without breaking schema-1 readers.

## Alternatives Considered
- Run benchdiff N−1 times and concatenate: no cross-implementation ranking; noisy comments.
- Delegate to benchstat / Bencher / github-action-benchmark: different hosting/service model and not a Markdown+Action-first drop-in; benchdiff keeps the single-binary, no-service property.
- Unicode/emoji-only charts (today's `▰`): works everywhere, but cannot express multi-series or distributions; kept as the fallback.
- Mermaid charts: GitHub renders `xychart-beta`, but limited styling and not usable in job summaries or standalone embeds consistently; evaluate in research.
- Inline `<svg>` in Markdown: stripped by GitHub; rejected as the sole path.

## Feasibility Risks
- Image hosting for comments may need extra permissions (`contents: write`, `actions`), fail on forks, or expose images publicly for private repos.
- SVG rendering differences across GitHub's camo proxy, dark mode, and mobile.
- Growth of `index.js`/WASM size and keeping the golden shim in sync.

## Observability Requirements
Standard logging via Action `core`-style log lines; the report states which SVGs were produced, which were skipped and why (cap, hosting failure), mirroring the existing "skipped metrics" surfacing. No metrics/alerts.

## Risk Control
Opt-in: all new behavior behind new inputs/flags/config (default off, so existing workflows are unchanged); golden tests guard the default path. Rollback = unset the inputs or pin the previous release tag (`@v0` floating tag stays on the last compatible release until verified). Ship under a minor version bump.

## Open Questions
- Which hosting mechanism for SVGs in comments and summaries (release asset vs artifact vs orphan branch vs other)? Research + ADR.
- Config file format: TOML vs JSON vs YAML (what the WASM core can parse with no new heavy deps)?
- Default dimension-extraction rules per input format, and the override syntax.
- Which charts ship in v1, and the multiple-comparison policy for N-way.
- Does the canonical JSON gain a `dims` map (schema 1 additive) or a schema 2?

*(unresolved after Phase 2 research — owner chose "accept and plan")*
- SVG hosting (orphan-branch raw URL) renders in comment and job summary: needs a real PR run before the ADR is final.
- Config format: stack research says TOML via `basic-toml` (+48 KB wasm); build-vs-buy says JSON-only. Plan decides.
- Multiple-comparison policy: architecture says none in v1; pitfalls/build-vs-buy say Holm vs the reference. Plan decides.

## Scope decisions recorded in the plan phase (2026-10-31)
- **Config format and discovery**: JSON (ADR-001), read only from an explicit `config` input / `--config`; no auto-discovery. The "e.g. `benchdiff.toml`/`.json`, checked into the repo" wording is satisfied by a checked-in file passed explicitly.
- **Per-metric thresholds**: kept in scope and wired into the pair kernel (plan story 1.2.5), not just parsed.
- **Comparison axis**: an extracted dimension, `run`, or the built-in pseudo-dimension `group`. **`measure` is cut as an axis** (explicit scope cut): different measures have different units and are `Incomparable` by construction, so pivoting on them has no meaningful comparison; `measure` always stays part of row identity.
- **Standalone SVG/HTML artifacts**: the standalone artifact in v1 is the `.svg` file itself (self-contained, opaque background, `<title>`/`<desc>`), exposed through `svg-dir`/`svg-paths` and attached to releases by the reusable workflow (plan story 5.1.4). **A standalone HTML wrapper page is cut from v1** (explicit scope cut): SVG opens directly in browsers and embeds in READMEs.
- **Importer-level dimensions** (JMH `@Param`, Criterion ids, Go `procs`) are a follow-up; v1 derives dimensions from names and labels only.
- **Config scope (round 3)**: thresholds are per `group/name` and per bare `group` (lookup `group/name` > `group` > default). "Output formats" in the Configuration bullet is read as the importer selection: `format` and `base-format` are config keys; the output paths `--markdown` and `--json` are not (explicit scope cut). `publish-images` and `svg-embed` are flag, key and input under one name each.
- **Chart embedding defaults (round 3)**: in the Action, a non-empty `charts` implies embedding and hosting unless `svg-embed: false` or `publish-images: false`; the CLI default is off. Opt-in is therefore preserved by `charts` defaulting to `none`, not by a second switch.
- **Failure classes (round 3)**: static configuration errors exit 2 (the Action fails the step); data-dependent problems (a rule matching nothing, a collision, an absent axis, a missing or empty variant file, a lost reference) degrade to a visible notice and never fail the job by default; `dimensions-strict` opts in to failing. Hosting and chart problems never fail the job.
- **Supporting work with no requirement line of its own (round 3)**: `--print-config` (supports AC-S4-2 precedence checks), the wasm size gate (supports the constraint that the Action ships a committed wasm and the freshness check that it matches source), and the 8-column cap (supports Non-functional scalability). They are tagged supporting work in the plan, not scope growth.

## Scope decisions recorded after triad review round 1 (2026-10-31)
- **N-way "regression" is defined** (ADR-005 item 7). The reference is the declared baseline; a matrix cell is `Regressed` only when it is significantly worse than the reference (same kernel, threshold and Holm correction as everywhere), rendered "slower than reference"; nothing is compared between non-reference variants. `fail-on-regression` in an N-way run requires an explicit `reference` (static exit 2 otherwise) and fails on a post-correction cell over the threshold or any invalid cell; without the flag an N-way run cannot fail on cell contents. The Action outputs `regressed`/`improved`/... keep their names and count cells versus the reference in matrix runs.
- **Vocabulary**: *variant* is the canonical term; `--variant` is the repeatable singular flag, `variants` the multi-line Action input (plan glossary, ux.md "Naming").
- **Added to existing scope, not new epics**: `cell-format` (compact ratio cells), `--print-dimensions` (a diagnostic like `--print-config`), a config `$schema` key with a published JSON Schema and a tested starter example (plan task 5.1.5a), hosting side-effect disclosure (README, `action.yml`, log line), notice ordering and cap, and the `chart-rows`/`max-rows` relationship. Each is a UX or documentation consequence of requirements already in scope.

## Scope decisions recorded after triad review round 2 (2026-10-31)
- **Falsifier baseline moved to the front** (plan task 0.3.1a/b, gate G0): see "Primary problem (falsifiable)". The 2-of-3-readers threshold is the same for baseline and treatment.
- **Reference and "fastest"** (ADR-005): the reference is the first input unless `reference` names another; "fastest" is only a per-row ranking marker, and reports say `has the best geomean` and `slower than reference`, never a head-to-head claim between non-reference variants, because only each variant against the reference is tested.
- **Adoption proxy**: a manual, telemetry-free count at release+30 and +90 days (see "Adoption proxy (no telemetry)").
- **Added to existing scope, not new epics**: a non-text contrast rule (3:1 for bars and markers) and a forced-colors/print note for charts, and the first-run permission guidance for `charts: delta` alone. Each is an accessibility or documentation consequence of the SVG requirement.

## Falsifier baseline result
Run 2026-11-01 (task 0.3.1b, gate G0). Reader: the owner only (single reader; the 2-reader requirement was not met, so confidence is low). Result: the owner reported missing at least one of Q1-Q3 or exceeding 60 seconds on today's pairwise output (self-reported; exact answers and seconds not recorded). Decision: `proceed`.
