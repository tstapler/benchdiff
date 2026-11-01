# ADR-001: Config file format is JSON in v1, loaded only from an explicit path

**Status**: Accepted
**Date**: 2026-10-31
**Project**: benchdiff-dimensions-svg

## Context
Research split. Stack research recommended TOML via `basic-toml` (+48 KB wasm over a serde_json control; 330,778 -> 395,336 total measured, see `research/stack.md`). Build-vs-buy and architecture research recommended JSON via the already-linked `serde_json` (+0 KB, `research/architecture.md` section 4.5). UX research prefers comments and table-shaped per-metric thresholds, which favours TOML for hand-editing.

## Decision
v1 ships **JSON only**, conventional file name `benchdiff.config.json`, parsed by `serde_json` into an all-`Option` `Config` struct with `#[serde(deny_unknown_fields)]` and a **required** `"version": 1` field (a missing `version` is an error; a canonical results file has `schema`, not `version`, and is rejected with that explanation). No new dependency.

The `Config` struct is format-agnostic and `config::parse_config(text, ConfigFormat)` takes a `ConfigFormat` enum (only `Json` in v1) so TOML (`basic-toml`) is an additive follow-up keyed on file extension, not a rewrite. Unknown keys are errors with a nearest-name suggestion (`unknown key 'chart-row'; did you mean 'chart-rows'?`). Keys use the same kebab-case names as Action inputs and CLI flags (`charts`, `compare-by`, `group-by`, `dimensions`, `dimensions-strict`, `reference`, `correction`, `thresholds`, `chart-rows`, `chart-width`, `chart-theme`, `svg-dir`, `svg-embed`, `publish-images`, `threshold`, `min-abs`, `sort`, `max-rows`, `only-changed`, `fail-on-regression`, `title`, `tag`, `format`, `base-format`, `cell-format`, plus the required `version` and the optional ignored `$schema` string (round 3 completes the list; the output paths `--markdown`/`--json` are deliberately not keys); `variant`/`variants` are deliberately not config keys, paths in a config file being relative to nothing; keys are added to the one key table by plan tasks 1.2.2a, 3.3.1a, 2.2.2a; a JSON Schema generated from that table and a tested starter example ship in task 5.1.5a).

Rationale: the wasm size budget is the scarcest shared resource (regex-lite already costs +55 KB, ADR-004); config is read by a CI action, so lack of comments is a mild annoyance, not a blocker; per-metric thresholds fit a JSON object keyed by `group/name` or a bare `group` (`{"thresholds":{"default":5,"group":4,"group/name":3}}`; lookup `group/name` > `group` > default), and are applied by the pair kernel via `Options::threshold_for` (plan task 1.2.5a), not merely parsed.

## Architecture constraints this decision carries (VERIFIED in `research/architecture.md` sections 0 and 4)
0. **No auto-discovery (amended after adversarial review).** The config is read only from an explicit `config` Action input / `--config` flag. `benchdiff.json` is a natural name for a committed canonical results file; auto-using it would hit `deny_unknown_fields`, exit 2, and fail a workflow that changed nothing, violating the opt-in / byte-identical guarantee. The conventional name `benchdiff.config.json` cannot collide with results files but is still not auto-used.
1. The wasm sees only the `/work` preopen and `env: {}`. It cannot read the config file from the repo. `index.js` must read the file host-side, copy it into the work dir via the existing `stage()` (as `/work/config/<its own sanitized base name>`, so error text and `--print-config` name the file the user wrote), and pass `--config /work/config/benchdiff.config.json`. Native CLI passes a real path to the same flag.
2. Layering is applied in exactly one place, the binary: `defaults < --config file < explicit flags`. Because the Action turns inputs into flags, "inputs over file" collapses into "flags over file".
3. `index.js` today always passes `--threshold`, `--min-abs`, `--sort` with JS defaults and `action.yml` declares `default:` for them, so an input can never be unset and would always beat the file. **Required change**: drop the `default:` for `threshold`, `min-abs`, `sort` (and any other config-governed input) in `action.yml`, document the default in `description`, and make `buildArgs` emit a flag only when the raw input is non-empty. `Options::default()` already equals the old defaults (threshold 5, min-abs 0, sort delta), so default output is unchanged. A `buildArgs` unit test asserts all-empty inputs emit no tunable flags.
4. A file `OptionsOverlay` (all `Option` fields) and a flag overlay are applied in order; this replaces flags mutating `Options` directly and removes the "flag equals default" ambiguity.

## Consequences
- No wasm growth from config parsing.
- No comments in config. Documented; a `"//"`-style key is NOT supported (deny_unknown_fields).
- `--print-config` prints one JSON object (`config-file`, `config`, `sources`) with the effective merged config and which layer set each non-default value. Source labels are exactly `default`, `config`, `flag`; because the Action turns inputs into flags (item 2), inputs and flags share the label `flag` (decision, round 3). The Method note uses the same labels (`threshold=3 (config)`).

## Alternatives rejected
- **TOML via `basic-toml`** (+48 KB, TOML 0.5, slow cadence): best ergonomics, but spends budget for a convenience. Revisit as a follow-up if users ask; the seam exists.
- **TOML via `toml` 1.1.8** (+96 KB): too large.
- **YAML**: `serde_yaml` deprecated; fork has a thin maintainer base.
- **json5**: pest-based, niche, unmeasured.

## Revisit trigger
Two or more user requests for comments in config, or wasm headroom above budget after Phase 3.
