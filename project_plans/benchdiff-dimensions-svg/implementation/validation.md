# Validation Plan: benchdiff-dimensions-svg

**Date**: 2026-10-31 (updated in round 3 / phase 4: gaps G-1..G-18 resolved or reassigned, new tests added for the consistency blockers and pre-mortem items; counts recomputed below)

Inputs: `requirements.md`, `implementation/plan.md` (Domain Glossary terms used for all type names), `design/ux.md` (48 acceptance criteria after triad round 2), ADR-001..007. Test names follow `methodName_should_ExpectedBehavior_When_Condition` (Rust: `snake_case`; JS: the same string passed to `test()`).

## Happy Path Scenario
Given today's baseline (two-way base/head only, Markdown + JSON output, 15 committed CLI golden cases plus the Action-level golden) and the fixture `testdata/nway/{quick,heap,merge}.txt` (Go bench output, 10 samples each, 6 benchmarks), when a maintainer runs the Action with `variants` of three labelled lines, `reference: quick`, `charts: ranking`, `svg-embed: true` on a public same-repo PR, then the sticky comment (marker first line) is exactly `testdata/nway/expected/matrix.md` with the reference column first, a bold fastest cell per row, a headline verdict line, a hosted `ranking.svg` image whose bytes are identical between native and WASM, and a collapsed text table; and the same run with no new inputs still reproduces every existing golden byte for byte.

## Requirement Register

IDs are assigned here because `requirements.md` has none. Every Success Metric, Constraint, Non-functional, Observability, Risk Control, In-Scope bullet and recorded scope decision has one ID.

| ID | Requirement (source section of requirements.md) |
|---|---|
| SM-1 | N >= 3 implementations side by side with a chosen reference, verified on a >= 3 fixture (Success Metrics 1) |
| SM-2 | Any dimension (impl, param, group) is a comparison axis, pivot without editing benchmarks (Success Metrics 2) |
| SM-3 | SVG renders correctly in PR comment, job summary and standalone file, verified by viewing; restated as a per-repo-class support matrix (public same-repo / no `contents: write` / fork / private / GHES / non-PR), each class verified in 5.1.3a or waived with a reason (Success Metrics 3, plan support matrix) |
| SM-4 | No new options: output byte-identical, goldens pass untouched (Success Metrics 4) |
| SM-5 | SVGs deterministic, light/dark readable, text alternatives (Success Metrics 5) |
| C-1 | Core is I/O-free Rust to WASM; Action stays zero-dependency `index.js` on `node24`, no run-time downloads (Constraints 1) |
| C-2 | SVG works in WASM/WASI sandbox, text metrics estimated not measured (Constraints 2) |
| C-3 | Hosted image delivery needs no secret beyond `GITHUB_TOKEN` (Constraints 3) |
| C-4 | Backward compatible: inputs/flags, schema 1 readers, outputs, sticky marker; additive schema (Constraints 4) |
| C-5 | All names/labels rendered into SVG are XML-escaped; untrusted-input hygiene extended (Constraints 5) |
| NF-1 | Performance SLO: < 1 s for 10k metrics, native and WASM (Non-functional 1) |
| NF-2 | SVG size bounded, explicit "N more not shown" (Non-functional 1) |
| NF-3 | Scalability to ~10 implementations x hundreds of benchmarks; larger inputs cap, never fail silently (Non-functional 2) |
| NF-4 | Security classification public; no data leaves the runner except via user's own PR/release (Non-functional 3, 4) |
| OB-1 | Report and log state which SVGs were produced/skipped and why (Observability) |
| RC-1 | Opt-in: new behaviour off by default; rollback by unsetting inputs (Risk Control) |
| SC-1 | Dimensions: identity generalized, params parsed from names, row key vs axis, sources: names and file labels (In Scope 1; Rabbit hole: explicit override) |
| SC-2 | N-way: N vs reference, ranking, relative deltas, significance per pair, Markdown matrix; two-way is the special case (In Scope 2; multiple-comparison policy) |
| SC-3 | SVG diagrams: delta/dumbbell, grouped ranking, optional distribution; light/dark safe, accessible, bounded (In Scope 3) |
| SC-4 | Delivery: `.svg` files, hosted-URL embed for comment and summary, standalone files, fallback with warning (In Scope 4) |
| SC-5 | Configuration: config file + inputs/flags, precedence flags > inputs > file > defaults, covers dimensions, reference, thresholds (per group and per metric), charts, sort, caps, input formats (In Scope 5) |
| SC-6 | Docs/README, unit + golden + SVG snapshot tests, rebuilt committed WASM (In Scope 6) |
| DEC-1 | Config is JSON, explicit path only, no auto-discovery (Scope decisions, ADR-001) |
| DEC-2 | Per-metric thresholds wired into the pair kernel (Scope decisions) |
| DEC-3 | Axis = extracted dim, `run`, or `group`; `measure` is not an axis (Scope decisions) |
| DEC-4 | Standalone artifact is the `.svg` file (svg-dir/svg-paths, release attach); HTML wrapper cut (Scope decisions) |
| DEC-5 | Importer-level dimensions out of v1; dims from names and labels only (Scope decisions) |
| DEC-6 | Config scope: thresholds per `group/name` and per `group`; `format`/`base-format` are keys; output paths are not (Scope decisions, round 3) |
| DEC-7 | Failure classes: static config errors exit 2; data-dependent problems warn by default, `dimensions-strict` fails; hosting/chart problems never fail the job (Scope decisions, round 3) |
| DEC-8 | Chart embedding defaults: Action `charts` non-empty implies embed and publish unless `svg-embed: false`/`publish-images: false`; CLI off (Scope decisions, round 3) |
| DEC-9 | N-way regression is defined (reference = declared baseline; `Regressed` only vs the reference); `fail-on-regression` needs an explicit `reference`; no flag means no failure on cell contents; vocabulary: variant is canonical (Scope decisions, triad round 1) |
| OM-1 | Outcome metric: a reader answers best-geomean-and-reference, is-it-significant and anything-slower-than-reference from the N-way comment alone in under 60 s (Success Metrics, Outcome metric; manual walkthrough) |
| OM-2 | Adoption proxy without telemetry: a manual count at release+30 and +90 days of the owner's repos (and public repos outside) whose workflows set `variants` or `charts` (requirements.md "Adoption proxy"; added in triad round 2) |
| FAL-1 | Falsifier baseline: today's pairwise output on `testdata/nway` is put to at least 3 readers (Q1-Q3, 2-of-3 under 60 s threshold) before the matrix and chart work starts (requirements.md "Primary problem", plan gate G0; added in triad round 2) |

Total: 34 requirements.

## Requirement → Test Mapping

Test file paths: Rust inline unit tests are `crates/benchdiff-core/src/<module>.rs` (shown as `<module>.rs`); Rust integration tests are `crates/benchdiff-core/tests/*.rs` or `crates/benchdiff/tests/*.rs`; JS tests are `test/*.test.js`.

| Requirement | Test File | Test Name | Type | Scenario |
|-------------|-----------|-----------|------|----------|
| SM-1: N>=3 side by side | matrix.rs | compare_matrix_should_emit_cells_for_all_variants_against_reference_when_three_variants | Unit | Happy path |
| SM-1: N>=3 side by side | matrix.rs | compare_matrix_should_fail_with_unknown_reference_listing_valid_labels_when_reference_not_a_label | Unit | Error path |
| SM-1: N>=3 side by side | matrix.rs | compare_matrix_should_give_identical_deltas_and_verdicts_when_inputs_permuted_with_explicit_reference | Unit | All permutations of `[quick, heap, merge]` with `reference: quick` |
| SM-1: N>=3 side by side | crates/benchdiff-core/tests/nway_fixture.rs | nway_fixture_should_render_identical_matrix_when_labelled_files_or_impl_subbenchmarks | Integration | `testdata/nway` three labelled files vs single-file `impl=` form equal `expected/matrix.md` and `matrix.json` |
| SM-1: N>=3 side by side | test/action.test.js | action_variants_should_post_matrix_comment_equal_to_expected_when_three_labelled_lines | Integration | Real `dist/benchdiff.wasm`, mocked fetch, marker first line (task 4.2.3c) |
| SM-2: any dimension as axis | dimensions.rs | extract_defaults_should_return_impl_and_size_dims_when_name_has_slash_kv_segments | Unit | Happy path |
| SM-2: any dimension as axis | matrix.rs | project_by_axis_should_split_variants_by_size_param_when_compare_by_size | Unit | Pivot on a param, not only impl |
| SM-2: any dimension as axis | matrix.rs | project_by_axis_should_error_no_benchmark_carries_dimension_when_axis_absent_everywhere | Unit | Error path |
| SM-2: any dimension as axis | crates/benchdiff/tests/matrix_cli.rs | cli_should_pivot_same_input_on_impl_then_size_without_editing_benchmarks | Integration | One file, two `--compare-by` runs, two different matrices |
| SM-3: renders in 3 targets | crates/benchdiff-core/src/svg/mod.rs | render_svgs_should_produce_self_contained_opaque_svg_when_delta_requested | Unit | Happy path (standalone target) |
| SM-3: renders in 3 targets | lib/images.js (test/images.test.js) | rewritePlaceholders_should_strip_image_and_warn_when_publish_fails | Unit | Error path (hosting unavailable) |
| SM-3: renders in 3 targets | test/action.test.js | main_should_embed_hosted_url_in_comment_and_job_summary_when_publisher_returns_urls | Integration | Both text targets receive the hosted URL |
| SM-3: renders in 3 targets | manual (G1, task 0.2.1b / 5.1.3a) | MAN-1 view hosted SVG in real PR comment, job summary, standalone file, plus the Camo `Content-Type`/`nosniff` header check and the seven embed variants | Integration | Human observation recorded in ADR-003; cannot be automated (task 0.2.1b) |
| SM-3: renders in 3 targets | manual (5.1.3a) | MAN-5 run the support-matrix variants A (public same-repo), B (no `contents: write`), C (fork PR), D (private repo), E (`workflow_dispatch`) and record observed vs expected per row in ADR-003; GHES by unit test | Integration | Human observation; any row not run is marked `waived: <reason>` |
| SM-3: renders in 3 targets | test/images.test.js | publishImages_should_return_the_documented_error_string_per_reason_and_make_no_fetch_when_gated | Unit | Fixed strings for private/fork/permission/push-failed/unsupported-host/no-event/disabled; zero `fetch` calls for the gated reasons |
| SM-4: byte-identical default | crates/benchdiff/tests/golden_expected.rs | golden_should_match_expected_bytes_when_no_new_options_for_all_fifteen_cli_cases | Unit | Happy path (native) |
| SM-4: byte-identical default | crates/benchdiff/tests/golden_expected.rs | golden_should_fail_naming_case_when_markdown_header_mutated | Unit | Error path (mutation detection) |
| SM-4: byte-identical default | test/golden.test.js | golden_should_match_expected_bytes_wasm_and_native_when_defaults | Integration | wasm == native == committed expected files |
| SM-4: byte-identical default | test/action.test.js | action_should_match_golden_when_results_file_named_benchdiff_json_in_workspace | Integration | Default path unaffected by stray results file |
| SM-4: byte-identical default | crates/benchdiff/tests/golden_expected.rs | golden_should_match_expected_bytes_for_each_importer_format_when_fixture_used_as_base_and_head | Unit | bmf, cargo-bench, criterion, gab-smaller, gab-bigger, googlebench, hyperfine, jmh (task 0.1.1a) |
| SM-4: byte-identical default | test/golden.test.js | action_golden_should_match_comment_body_outputs_and_failed_flag_when_no_tunable_inputs | Integration | `main()` through the real wasm vs `expected/action-default.*` (guards the `action.yml` default removal) |
| SM-4: byte-identical default | crates/benchdiff/tests/config_cli.rs | binary_defaults_should_equal_removed_action_yml_defaults_table_with_explicit_exceptions | Unit | format auto, threshold 5, min-abs 0, sort delta, title, tag, only-changed, fail-on-regression, max-rows (task 1.2.4a) |
| SM-5: deterministic, themed, accessible | crates/benchdiff-core/tests/svg_security.rs | render_svgs_should_be_byte_identical_when_rendered_twice | Unit | Happy path |
| SM-5: deterministic, themed, accessible | svg/style.rs | card_should_have_role_img_title_desc_and_opaque_background_when_any_chart | Unit | Accessibility + opaque card |
| SM-5: deterministic, themed, accessible | svg/style.rs | render_svgs_should_error_when_theme_unknown_enum | Unit | Error path |
| SM-5: deterministic, themed, accessible | test/golden.test.js | svg_should_be_byte_identical_native_and_wasm_when_nway_charts | Integration | Native vs WASM (4.2.2a) |
| C-1: I/O-free core, zero-dep Action | crates/benchdiff-core/tests/purity.rs | core_should_not_use_fs_net_process_env_when_sources_scanned | Unit | Happy path: scan of `crates/benchdiff-core/src/**` finds no `std::fs`/`std::net`/`std::process`/`std::env` outside `#[cfg(test)]` modules (task 0.1.2a); a planted use fails naming the file |
| C-1: I/O-free core, zero-dep Action | crates/benchdiff-core/tests/purity.rs | core_purity_scan_should_ignore_std_fs_inside_cfg_test_modules_and_fail_outside_them | Unit | Edge/error path: `import/mod.rs:137` and `import/json_formats.rs:425` (inside `#[cfg(test)]`) pass; the same call outside a test module fails naming file and line (task 0.1.2a) |
| C-1: I/O-free core, zero-dep Action | test/action.test.js | package_json_should_be_absent_or_have_no_dependencies_and_index_and_lib_should_require_only_node_builtins | Unit | Error path guard: there is no `package.json` today (absence passes); a temp-dir `package.json` with a non-empty `dependencies` fails naming it; a non-`node:` `require` in `index.js`/`lib/*.js` fails (task 0.1.3a) |
| C-1: I/O-free core, zero-dep Action | test/images.test.js | images_module_should_require_only_node_builtins_when_loaded | Integration | `lib/images.js` imports only `node:*` modules |
| C-2: WASI sandbox, estimated text metrics | svg/text.rs | text_width_should_use_advance_table_with_ten_percent_margin_when_ascii_label | Unit | Happy path |
| C-2: WASI sandbox, estimated text metrics | svg/text.rs | text_width_should_count_one_em_when_cjk_char | Unit | Error/edge path (non-ASCII) |
| C-2: WASI sandbox, estimated text metrics | test/golden.test.js | wasm_should_render_svg_without_fonts_or_extra_preopens_when_only_work_dir_mounted | Integration | WASI run with `env: {}` and `/work` preopen only |
| C-3: only GITHUB_TOKEN | test/images.test.js | publishImages_should_call_git_data_api_with_github_token_only_when_public_same_repo | Unit | Happy path |
| C-3: only GITHUB_TOKEN | test/images.test.js | publishImages_should_return_permission_reason_when_blob_create_403 | Unit | Error path |
| C-3: only GITHUB_TOKEN | test/images.test.js | publishImages_should_create_orphan_branch_commit_and_return_sha_pinned_url_when_mock_fetch | Integration | blob/tree/commit/ref sequence, no other credential read |
| C-4: backward compatible | model.rs | metric_should_roundtrip_bytes_when_json_has_no_dims | Unit | Happy path |
| C-4: backward compatible | model.rs | parse_should_reject_schema_two_when_reader_is_schema_one | Unit | Error path (unchanged behaviour) |
| C-4: backward compatible | crates/benchdiff-core/tests/json_compat.rs | render_json_should_equal_expected_default_json_when_two_way_and_matrix_key_absent | Integration | Schema stays 1, no `matrix` key |
| C-4: backward compatible | test/action.test.js | buildArgs_should_emit_no_tunable_flags_when_inputs_empty_and_keep_existing_outputs | Integration | Existing outputs/marker intact |
| C-4: backward compatible | test/action.test.js | exports_should_still_resolve_runWasm_stage_expand_upsertComment_clip_when_index_required | Integration | Existing exports retained |
| C-5: XML escaping | svg/writer.rs | xml_escape_should_encode_markup_chars_when_script_payload | Unit | Happy path |
| C-5: XML escaping | svg/writer.rs | xml_escape_should_replace_control_chars_with_fffd_when_nul_or_backspace | Unit | Error path |
| C-5: XML escaping | crates/benchdiff-core/tests/svg_security.rs | every_chart_should_be_well_formed_and_free_of_script_href_url_when_hostile_name_corpus | Integration | Corpus across delta/ranking/distribution |
| C-5: XML escaping | markdown_matrix.rs | render_matrix_markdown_should_escape_hostile_names_when_untrusted_fields | Integration | Markdown path |
| NF-1: perf SLO | crates/benchdiff-core/tests/perf.rs | perf_two_way_should_finish_under_one_second_when_ten_thousand_metrics_release | Unit | Happy path (`make perf`) |
| NF-1: perf SLO | crates/benchdiff-core/tests/perf.rs | perf_nway_should_finish_under_one_second_when_ten_variants_300_benchmarks_with_holm | Unit | N-way worst case incl. extraction + SVG |
| NF-1: perf SLO | test/perf.test.js | perf_wasm_should_fail_only_above_two_seconds_and_log_time_when_nway_input | Integration | WASM guard |
| NF-2: SVG size bounded | svg/mod.rs | render_svgs_should_stay_under_100kb_when_ten_variants_30_rows | Unit | Happy path |
| NF-2: SVG size bounded | svg/delta.rs | delta_chart_should_cap_rows_and_print_n_more_not_shown_when_47_rows_cap_30 | Unit | Error/edge path: footer `+ 17 more not shown` and `CapExceeded{30,47}` |
| NF-2: SVG size bounded | test/action.test.js | main_should_warn_in_log_when_svg_exceeds_100kb | Integration | Size warning surfaced |
| NF-3: scalability, cap not fail | svg/ranking.rs | ranking_chart_should_list_ninth_series_in_footer_when_nine_variants | Unit | Happy path |
| NF-3: scalability, cap not fail | markdown_matrix.rs | render_matrix_markdown_should_show_eight_columns_and_notice_when_eleven_variants | Unit | Error path: overflow is visible |
| NF-3: scalability, cap not fail | crates/benchdiff-core/tests/nway_scale.rs | nway_should_complete_and_report_caps_when_ten_variants_hundreds_of_rows | Integration | Never fails silently |
| NF-4: public / no data egress | test/images.test.js | publishImages_should_only_contact_github_api_host_when_any_path | Unit | Happy path: asserts every mocked `fetch` URL host is the GitHub API host |
| NF-4: public / no data egress | test/images.test.js | publishImages_should_not_call_fetch_when_repo_private | Unit | Error path: private repo, zero network writes |
| NF-4: public / no data egress | test/action.test.js | main_should_make_no_network_calls_when_charts_unset | Integration | Default path sends nothing except the existing comment upsert |
| OB-1: produced/skipped surfaced | svg/mod.rs | skip_reason_should_be_recorded_when_chart_kind_not_applicable_to_report_kind | Unit | Happy path (`NotApplicable`) |
| OB-1: produced/skipped surfaced | markdown.rs | render_markdown_with_should_list_skipped_charts_with_reason_when_extras_contain_skips | Unit | Error path |
| OB-1: produced/skipped surfaced | test/action.test.js | main_should_log_charts_requested_produced_skipped_line_when_charts_delta_ranking_on_two_way | Integration | `charts: requested=delta,ranking produced=1 skipped=1 (...)` |
| RC-1: opt-in, rollback | options.rs | options_overlay_should_equal_default_when_no_flags_or_config | Unit | Happy path |
| RC-1: opt-in, rollback | main.rs (crates/benchdiff/tests/config_cli.rs) | cli_should_exit_two_with_message_when_config_error_and_not_when_data_error | Unit | Failure policy: config fails fast |
| RC-1: opt-in, rollback | test/action.test.js | main_should_return_original_comment_when_charts_unset_after_charts_run | Integration | Unset restores baseline (also AC-X-3) |
| SC-1: dimensions | dimensions.rs | extract_defaults_should_keep_dash_suffix_whole_when_size_10_dash_8 | Unit | Happy path (ADR-004 item 3) |
| SC-1: dimensions | dimensions.rs | rule_parse_should_reject_pattern_without_named_group_or_over_256_chars | Unit | Error path |
| SC-1: dimensions | dimensions.rs | apply_should_fail_with_matched_zero_of_n_and_example_when_rule_matches_nothing | Unit | Error path (exit 2 text) |
| SC-1: dimensions | dimensions.rs | apply_should_name_both_originals_when_residual_identity_collides | Unit | Error path |
| SC-1: dimensions | matrix.rs | project_by_axis_should_label_rows_with_non_axis_dims_when_two_dimensions | Unit | `Sort [size=10]` |
| SC-1: dimensions | crates/benchdiff-core/tests/dimensions_import.rs | apply_should_extract_dims_from_every_importer_fixture_without_procs_when_defaults | Integration | Go/JMH/Criterion/GBench fixtures |
| SC-1: dimensions | dimensions.rs | dimension_summary_should_list_dims_first_seen_values_and_counts_plus_group_pseudo_dimension | Unit | Zero-config rules (`/k=v`, `[k=v]`, `(k = v)`); names with no `=` give `group` only |
| SC-1: dimensions | crates/benchdiff/tests/matrix_cli.rs | cli_print_dimensions_should_emit_json_summary_and_exit_zero_without_report | Integration | `--print-dimensions` with `--head` and with `--variant` files; no report, no other writes |
| SC-2: N-way vs reference | matrix.rs | compare_matrix_should_mark_regressed_and_improved_cells_when_130_and_80_vs_100 | Unit | Happy path |
| SC-2: N-way vs reference | matrix.rs | compare_matrix_should_mark_missing_cell_not_zero_when_variant_lacks_benchmark | Unit | Error path |
| SC-2: N-way vs reference | matrix.rs | compare_matrix_should_return_direction_mismatch_without_calling_kernel_when_better_differs | Unit | Error path |
| SC-2: N-way vs reference | matrix.rs | compare_matrix_should_return_invalid_cell_and_trip_failed_when_variant_series_all_nan | Unit | Error path |
| SC-2: N-way vs reference | matrix.rs | compare_matrix_should_list_reference_absent_rows_without_deltas_when_reference_lacks_benchmark | Unit | Error path |
| SC-2: N-way vs reference | stats.rs | holm_adjust_should_equal_textbook_vector_when_p_0_01_0_04_0_03 | Unit | Happy path |
| SC-2: N-way vs reference | matrix.rs | holm_pass_should_only_downgrade_and_use_m_equal_cells_with_pvalue_when_floor_suppressed_cell | Unit | ADR-002 family definition |
| SC-2: N-way vs reference | matrix.rs | holm_pass_should_be_identity_when_single_variant | Unit | m=1 identity |
| SC-2: N-way vs reference | matrix.rs | counts_geomean_and_matrix_failed_should_use_post_holm_verdicts_when_cell_downgraded | Unit | Ordering |
| SC-2: N-way vs reference | matrix.rs | geomean_should_be_orientation_aware_and_exclude_zero_with_n_used_of_total | Unit | Geomean |
| SC-2: N-way vs reference | matrix.rs | geomean_should_not_mix_measures_and_hide_family_under_three_rows | Unit | Families |
| SC-2: N-way vs reference | matrix.rs | geomean_headline_should_use_common_row_set_with_per_variant_n_used_of_total_when_variant_misses_rows | Unit | `merge` lacks 6 slowest of 15: headline over 9 common rows, per-variant `9/15` and `15/15` |
| SC-2: N-way vs reference | markdown_matrix.rs | render_matrix_markdown_should_state_reference_and_print_not_comparable_when_common_set_under_half | Unit | Headline `reference: quick (first listed; ...)` line; `not comparable: merge covers 4/15 benchmarks` |
| SC-2: N-way vs reference | markdown_matrix.rs | render_matrix_markdown_should_render_em_dash_for_missing_and_warning_invalid_for_invalid | Unit | Snapshot: Missing `—` (U+2014, never `-`/0), Invalid `⚠️ invalid` |
| SC-2: N-way vs reference | matrix.rs | matrix_should_not_fail_when_300_identical_distribution_rows | Unit | Seeded generator, CV <= 2%, default threshold: `matrix_failed` false (pins the threshold floor across rows; Holm is per-row only) |
| SC-2: N-way vs reference | crates/benchdiff/tests/matrix_cli.rs | cli_should_degrade_with_notice_when_rule_matches_nothing_and_exit_two_only_with_dimensions_strict | Integration | Zero-match, axis-absent and absent-reference cases: notice + two-way/notice-only fallback exit 0; strict exit 2 |
| SC-2: N-way vs reference | matrix.rs | project_by_axis_should_return_notice_not_error_when_reference_absent_from_axis_values_and_not_strict | Unit | Data-dependent reference problem |
| SC-2: N-way vs reference | dimensions.rs | apply_should_keep_first_and_note_both_originals_when_collision_and_not_strict | Unit | Non-strict collision policy (strict case already covered by the collision test above) |
| SC-2: N-way vs reference | test/action.test.js | action_variants_should_drop_empty_variant_post_matrix_and_pass_when_not_strict | Integration | `heap` file empty: drop notice, remaining matrix, comment posted, job passes (tasks 4.2.3b and 4.2.3c) |
| SC-2: N-way vs reference | test/action.test.js | action_variants_should_post_notice_only_comment_without_running_binary_when_reference_variant_empty | Integration | Lost reference or < 2 variants; `dimensions-strict` fails the step instead |
| SC-2: N-way vs reference | matrix.rs | ranking_should_follow_reference_direction_when_better_lower_then_higher | Unit | Ranking |
| SC-2: N-way vs reference | crates/benchdiff-core/tests/json_compat.rs | render_json_with_matrix_should_emit_companion_counts_summing_to_variants_minus_one_times_rows | Integration | Invariant from ADR-006 |
| SC-2: N-way vs reference | crates/benchdiff/tests/matrix_cli.rs | cli_should_exit_one_when_regressed_cell_over_fail_on_regression_with_explicit_reference_or_invalid_cell | Integration | Failure semantics (DEC-9: explicit reference required) |
| SC-2: N-way vs reference | crates/benchdiff/tests/matrix_cli.rs | cli_should_keep_two_way_goldens_when_no_variant_flag | Integration | Two-way is the untouched special case |
| SC-2: N-way vs reference | matrix.rs | matrix_failed_should_be_false_without_gate_and_true_only_for_post_holm_regressed_over_threshold_or_invalid | Unit | DEC-9: `None` -> false; `Some(10)` with +7% -> false, +30% -> true, Invalid -> true |
| SC-2: N-way vs reference | matrix.rs | project_by_axis_should_default_reference_to_first_encountered_and_be_deterministic_when_variants_split_from_names | Unit | Order `heap, quick, merge` gives reference `heap`; same input twice gives same order |
| SC-2: N-way vs reference | matrix.rs | project_by_axis_should_end_axis_errors_with_available_dimensions_when_axis_absent_unknown_or_measure | Unit | `compare-by` absent, `group-by` unknown, `compare-by measure`; suffix from `available_dimensions_text()` |
| SC-2: N-way vs reference | crates/benchdiff/tests/matrix_cli.rs | cli_should_exit_two_when_fail_on_regression_without_explicit_reference_in_nway | Integration | DEC-9 static error text; same with explicit reference exits 1 on +30%, 0 on +7%; gate skipped when explicit reference lost |
| SC-2: N-way vs reference | markdown_matrix.rs | render_matrix_markdown_should_print_gate_line_and_never_use_regress_word_in_prose | Unit | `gate: fail-on-regression 10% vs quick: passed or FAILED (...)`; regex scan for `regress` finds only the flag name |
| SC-2: N-way vs reference | markdown_matrix.rs | render_matrix_markdown_should_render_legend_line_directly_under_table_with_only_used_glyphs | Unit | AC-S1-10: snapshot of the full legend line, including the always-present clause that bold and Rank are untested orderings (only variant vs reference is tested) |
| SC-2: N-way vs reference | markdown_matrix.rs | render_matrix_markdown_should_say_best_geomean_not_fastest_and_count_slower_than_reference_rows | Unit | Headline `has the best geomean (...; variants are not tested against each other); slower than reference in 2 of 15 rows`; `0 of 15` is valid; reference-is-best variant `**quick** (the reference) has the best geomean...`; scan finds no `fastest`/`loses`/`higher`/`lower`/`regress` in prose |
| SC-2: N-way vs reference | markdown_matrix.rs | render_matrix_markdown_should_use_summary_tag_title_max_rows_and_only_changed | Unit | Presentation settings reach the renderer through `MatrixSummary` only (2.2.1a): marker from `tag`, heading from `title`, `…and 27 more rows not shown (`max-rows: 20`)…`, `N rows with no significant difference hidden (`only-changed`)` |
| SC-2: N-way vs reference | matrix.rs | compare_matrix_should_copy_options_and_matrix_options_into_summary_so_renderers_need_no_second_argument | Unit | 2.1.3a: `Options` + `MatrixOptions` -> `MatrixSummary` (title, tag, max_rows, only_changed, fail_on_regression_pct, cell_format, columns_cap); chart cap `min(chart-rows, max-rows)` computable from the report |
| SC-2: N-way vs reference | markdown_matrix.rs | gate_line_should_name_three_cells_cut_each_to_forty_chars_cap_at_two_hundred_and_list_all_in_method_when_seven_fail | Unit | 2.2.1b: `FAILED (7 cells: ... and 4 more; full list in Method)`; Invalid reads `merge on Sort invalid`; Method lists the first 50 |
| SC-2: N-way vs reference | crates/benchdiff-core/tests/nway_fixture.rs | nway_matrix_should_keep_q1_q3_answerable_and_baseline_pairwise_files_should_still_match_the_two_way_path | Unit | 2.2.3a: at least 3 rows with a variant significantly better than the reference, at least 1 cell slower than the reference, at least 1 `~` row; the committed `testdata/nway/baseline/pairwise-*.md` (task 0.3.1a) still equal today's two-way CLI output |
| SC-2: N-way vs reference | test/action.test.js | action_notice_only_body_built_in_js_should_equal_expected_notice_only_md_that_the_rust_snapshot_also_asserts | Integration | 4.2.3b: one golden file (`testdata/nway/expected/notice-only.md`) shared by 2.2.1b's Rust snapshot and the JS mirror, so they cannot drift |
| SC-5: configuration | compare.rs / markdown.rs | summary_should_serialize_identically_and_method_note_should_print_nothing_extra_when_overrides_and_sources_empty | Unit | 1.2.1b: default `render_json` and `render_markdown` bytes equal `expected/default.*` with the two new `Summary` fields present but empty |
| SC-5: configuration | markdown.rs | render_method_should_list_threshold_overrides_and_non_default_sources_when_present | Unit | 1.2.1b/1.2.5a/1.2.3a: `Per-metric thresholds: net/Parse 20%.` and `Settings not at their defaults: threshold=3 (config), sort=name (flag).` |
| SC-2: N-way vs reference | markdown_matrix.rs | render_matrix_markdown_should_render_compact_ratio_cells_when_cell_format_ratio | Unit | `0.74x 🟢`; reference column keeps absolute value; `full` unchanged |
| SC-2: N-way vs reference | markdown_matrix.rs | render_notices_should_order_by_class_cap_at_five_and_never_precede_the_verdict | Unit | 7 notices: first 5 in class order, then `⚠️ 2 more notices (see Method)`; Method lists all; notice-only report starts with first notice |
| SC-3: SVG diagrams | svg/delta.rs | delta_chart_should_draw_diverging_bars_with_shapes_and_value_labels_when_three_rows | Unit | Happy path |
| SC-3: SVG diagrams | svg/delta.rs | delta_chart_should_clip_at_100_percent_with_arrow_and_true_label_when_450_percent | Unit | Edge/error path |
| SC-3: SVG diagrams | svg/ranking.rs | ranking_chart_should_use_log2_scale_gridline_and_hollow_slot_when_missing_cell | Unit | Happy/edge path |
| SC-3: SVG diagrams | svg/distribution.rs | distribution_chart_should_draw_lanes_with_median_tick_when_ten_samples | Unit | Happy path |
| SC-3: SVG diagrams | svg/distribution.rs | distribution_chart_should_skip_with_no_samples_when_keep_samples_false | Unit | Error path |
| SC-3: SVG diagrams | compare.rs | compare_should_scale_base_samples_to_head_unit_when_keep_samples_and_us_vs_ns | Unit | Samples unit scale |
| SC-3: SVG diagrams | svg/style.rs | palette_should_have_3_to_1_non_text_contrast_by_fill_or_outline_for_light_and_dark_palettes | Unit | AC-S2-11: computes `#d03b3b` 4.68, `#0ca30c` 3.27, gray `#8a8a86` 3.38; series 3-5 fail as fills and must carry the `#52514e` outline; fails if the green drops under 3:1 |
| SC-3: SVG diagrams | svg/style.rs | light_card_should_contain_no_style_media_query_or_system_color_keyword | Unit | AC-S2-12: every colour is a literal hex; forced-colors/print limit documented in the README (docs test) |
| SC-3: SVG diagrams | crates/benchdiff-core/tests/svg_geometry.rs | every_text_should_fit_its_container_and_no_label_boxes_should_overlap_when_worst_case_set | Unit | 500-char, CJK, RTL, 8 and 10 variants x 30 rows |
| SC-3: SVG diagrams | crates/benchdiff-core/tests/svg_geometry.rs | geometry_assertion_should_fail_when_label_overflows_its_container_fixture | Unit | Error path: the assertion can fail |
| SC-3: SVG diagrams | crates/benchdiff-core/tests/json_compat.rs | json_should_exclude_samples_when_keep_samples_true | Integration | G-18: `#[serde(skip)]` `Samples` never reach default or matrix JSON |
| SC-3: SVG diagrams | crates/benchdiff-core/tests/snapshots/svg (svg_snapshots.rs) | svg_snapshots_should_match_committed_files_when_nway_and_two_way_fixtures | Integration | Snapshot per chart kind and theme |
| SC-4: delivery | crates/benchdiff/tests/svg_cli.rs | cli_should_write_svg_and_list_path_on_stderr_when_charts_delta_svg_dir | Unit | Happy path |
| SC-4: delivery | crates/benchdiff/tests/svg_cli.rs | cli_should_print_report_then_exit_two_naming_dir_when_svg_dir_unwritable | Unit | Error path |
| SC-4: delivery | lib/images.js (test/images.test.js) | rewritePlaceholders_should_replace_every_benchdiff_svg_token_when_picture_block | Unit | `<picture>` pair |
| SC-4: delivery | test/images.test.js | clip_should_move_cut_before_image_or_picture_markup_when_cut_inside | Unit | Truncation with images |
| SC-4: delivery | markdown.rs | render_markdown_with_should_emit_chart_data_text_details_after_image_when_embedded_and_none_when_no_chart | Unit | Two-way (G-1): `<details><summary>Chart data (text)</summary>` with the chart's rows, cap line, escaped names |
| SC-4: delivery | markdown_matrix.rs | render_matrix_markdown_should_emit_chart_data_text_details_with_em_dash_for_missing_when_embedded | Unit | N-way (G-1) |
| SC-4: delivery | test/images.test.js | rewritePlaceholders_should_open_chart_data_details_and_put_warning_in_markdown_body_when_publish_fails | Unit | `<details>` becomes `<details open>`; `⚠️` line in the body, not only the log |
| SC-4: delivery | crates/benchdiff/tests/svg_cli.rs | cli_should_embed_only_when_svg_embed_true_or_auto_with_charts_and_respect_concrete_config_false | Unit | `auto` is a default-layer value: charts+auto embeds, config false beats auto, bare `--charts` does not embed |
| SC-4: delivery | crates/benchdiff/tests/svg_cli.rs | cli_should_emit_no_placeholder_and_note_line_when_publish_images_false | Unit | `charts not embedded: publish-images is false; ...` plus opened text block |
| SC-4: delivery | test/action.test.js | main_should_embed_and_publish_by_default_when_charts_set_alone | Integration | AC-S5-4: hosted image in comment and summary with only `charts: delta` |
| SC-4: delivery | test/action.test.js | main_should_write_files_only_and_make_no_publish_calls_when_svg_embed_false | Integration | Opt-out; `svg-paths` still set |
| SC-4: delivery | test/action.test.js | main_should_keep_text_chart_and_skip_publish_when_publish_images_false | Integration | Opt-out; job passes |
| SC-4: delivery | test/images.test.js | publishImages_should_create_no_commit_when_content_hash_paths_already_at_branch_tip | Unit | Retention: identical bytes twice, second run no ref update |
| SC-4: delivery | test/images.test.js | publishImages_should_never_delete_or_force_push_and_keep_previous_tip_reachable_when_publishing | Unit | Retention: history preserved; rollback advice is to unset `charts` |
| SC-4: delivery | test/action.test.js | main_should_expose_svg_dir_and_svg_paths_outputs_when_charts_delta | Integration | Standalone files |
| SC-4: delivery | test/action.test.js | release_workflow_should_upload_svg_paths_and_never_set_publish_images_when_charts_input | Integration | `actionlint` + step-list diff of release-benchmarks.yml (5.1.4a) |
| SC-4: delivery | svg/mod.rs | chart_rows_and_max_rows_should_agree_on_shown_rows_and_n_more_counts | Unit | 47 changed rows: `max-rows 20`/`chart-rows 30` equal counts and rows; `max-rows` unset 30 shown; `max-rows 40`/`chart-rows 30` each line names its own key |
| SC-4: delivery | svg/mod.rs | select_rows_should_order_by_abs_delta_prefix_position_labels_and_match_text_table_numbers_when_sort_name | Unit | 3.2.4a: 47 rows, `sort=name`: chart label `1.` is the largest-|delta| row, the text block `#` column equals the chart numbers, footer `largest change first`; the main table is untouched |
| SC-4: delivery | svg/mod.rs | chart_text_table_should_emit_ratio_columns_dash_for_missing_and_cap_line_when_ranking_and_median_range_when_distribution | Unit | 3.3.2a: N-way fallback content: `# | Benchmark | quick (ref) | heap | merge`, `0.74x 🟢`, `—`, `⚠️ invalid`, reference-absent rows counted in one line, `+ N more not shown (chart-rows: 30)`, one block per chart with `Chart data (text): <kind>` titles |
| SC-4: delivery | svg/mod.rs | alt_text_should_name_verdict_and_top_change_truncate_name_first_and_stay_within_125_chars_and_equal_title | Unit | 3.3.2a: two-way, N-way, all-unchanged and a 200-char name; no `higher`/`lower`/`regress` |
| SC-4: delivery | svg/mod.rs | render_svgs_should_skip_with_nothing_to_compare_and_follow_the_kind_versus_report_kind_table | Unit | 3.3.1b (moved from 3.2.4): head-only run; `delta` on N-way and `ranking` on two-way give `NotApplicable`, `produced=1 skipped=1` |
| SC-4: delivery | test/images.test.js | rewritePlaceholders_should_name_directory_and_upload_artifact_hint_and_add_permissions_pointer_when_permission | Unit | 4.1.1a: `SVGs are in <svg-dir>; to keep them, upload that directory with actions/upload-artifact`, never an artifact name; `permission` line points to the README snippet |
| SC-4: delivery | test/action.test.js | main_should_post_permission_line_with_snippet_pointer_open_text_block_and_pass_when_charts_delta_alone_meets_read_only_token | Integration | 4.2.1b: first-run case; `svg-paths` still set; snippet printed once in the log |
| SC-4: delivery | crates/benchdiff/tests/svg_cli.rs | help_should_list_chart_flags_with_default_and_allowed_values_and_state_effective_cap | Integration | 3.3.1a: `--charts ... --publish-images` listed with `default`; `--chart-rows` states `min(chart-rows, max-rows)` |
| SC-4: delivery | test/action.test.js | action_yml_should_describe_hosting_side_effects_and_log_one_line_before_first_publish | Integration | AC-S4-5: `contents: write`, `benchdiff-assets`, retention, public URL, private rule; log line precedes the first write |
| SC-4: delivery | test/docs.test.js | readme_should_disclose_hosting_regression_definition_naming_dimension_rules_and_accessibility_path | Unit | README content assertions (5.1.1 triad round 1 additions 1-11 and round 2 additions 12-17: untested orderings, upload-artifact hint and first-run permissions snippet, chart/table order and `#` aid, forced-colors/print and 3:1 note, gate truncation, adoption proxy) |
| SC-4: delivery | test/action.test.js | action_variants_should_degrade_with_warning_when_glob_matches_zero_files_and_fail_when_more_than_one | Integration | One rule: 0 matches warns and drops the variant, >1 fails the step |
| SC-5: configuration | config.rs | parse_config_should_map_valid_file_to_overlay_when_threshold_sort_thresholds | Unit | Happy path |
| SC-5: configuration | config.rs | parse_config_should_suggest_nearest_key_when_treshold_typo | Unit | Error path |
| SC-5: configuration | config.rs | parse_config_should_list_valid_enum_values_when_sort_size | Unit | Error path |
| SC-5: configuration | config.rs | parse_config_should_error_missing_version_and_newer_version | Unit | Error path |
| SC-5: configuration | config.rs | parse_config_should_accept_format_and_base_format_and_list_valid_formats_when_unknown | Unit | Config keys `format`/`base-format` (B3) |
| SC-5: configuration | config.rs | parse_config_should_report_file_line_column_when_json_malformed_or_comment_present | Unit | `benchdiff.config.json: line 2 column 3: ...` (AC-S5-2, G-4) |
| SC-5: configuration | crates/benchdiff/tests/config_cli.rs | help_should_list_every_new_flag_with_default_and_allowed_values_from_key_table | Integration | G-5: generated from the key table |
| SC-5: configuration | crates/benchdiff/tests/config_cli.rs | print_config_should_emit_json_with_config_file_config_and_sources_using_default_config_flag_labels | Integration | Decided format (G-6) |
| SC-5: configuration | crates/benchdiff/tests/vocabulary.rs | vocabulary_should_match_across_action_yml_inputs_cli_flags_and_config_keys_with_explicit_allowlists | Integration | G-8: set equality except the listed exceptions, with allowlists matched to the real files (inputs `comment`, `github-token`, `variants` are action-only; flags `--alpha`, `--no-stats`, `--markdown`, `--json`, `--head-json`, `--variant`, `--print-config`, `--print-dimensions`, `--help` are CLI-only; `version`, `$schema`, `thresholds` are config-only); fails when an allowlisted name no longer exists |
| SC-5: configuration | options.rs | overlay_should_apply_defaults_then_file_then_flags_when_all_three_set | Unit | Precedence |
| SC-5: configuration | crates/benchdiff/tests/config_cli.rs | cli_should_use_flag_over_file_value_and_print_sources_when_print_config | Integration | `--config` layering |
| SC-5: configuration | test/action.test.js | buildArgs_should_stage_config_and_pass_flag_only_when_config_input_set | Integration | Action layering |
| SC-5: configuration | config.rs | parse_config_should_accept_schema_key_as_ignored_string_and_reject_non_string | Unit | `$schema` accepted; `"$schema": 3` -> `$schema: expected a string`; not echoed by `--print-config` |
| SC-5: configuration | crates/benchdiff/tests/config_schema.rs | schema_should_equal_committed_file_generated_from_key_table | Integration | Fails naming a key added without regenerating `benchdiff.config.schema.json` |
| SC-5: configuration | crates/benchdiff/tests/config_schema.rs | starter_example_should_parse_run_and_validate_against_schema | Integration | `examples/benchdiff.config.json` on `testdata/nway/impl.txt`; exit 0, matrix, `--print-config` sources `config` |
| SC-5: configuration | crates/benchdiff/tests/config_schema.rs | starter_example_should_fail_with_line_and_column_when_regex_backslash_not_doubled | Unit | Single-backslash variant is an invalid JSON escape |
| SC-6: docs/tests/wasm | scripts/check-wasm-size.sh (test/wasm_size.test.js) | wasm_size_gate_should_print_size_and_exit_zero_when_330650_bytes | Unit | Happy path |
| SC-6: docs/tests/wasm | scripts/check-wasm-size.sh (test/wasm_size.test.js) | wasm_size_gate_should_exit_one_when_500000_bytes | Unit | Error path |
| SC-6: docs/tests/wasm | CI job wasm-fresh (scripts/check-wasm-fresh.sh) | wasm_fresh_should_fail_when_committed_wasm_differs_from_clean_rebuild | Integration | Runs on PRs touching `dist/` and release tags; reproducibility resolved in W1 (task 0.1.2a) |
| SC-6: docs/tests/wasm | test/docs.test.js | readme_should_state_holm_per_row_only_embed_defaults_support_matrix_and_phone_note_when_read | Unit | Round-3 README content assertions (5.1.1) |
| SC-6: docs/tests/wasm | test/golden.test.js | committed_wasm_should_match_native_output_when_all_golden_and_nway_cases | Integration | Rebuilt wasm in sync |
| SC-6: docs/tests/wasm | test/docs.test.js | readme_should_contain_vocabulary_table_precedence_and_permissions_sections_when_read | Integration | Doc content assertions (5.1.1) |
| DEC-1: JSON, explicit path | config.rs | parse_config_should_reject_results_file_with_schema_key_when_passed_by_mistake | Unit | Error path |
| DEC-1: JSON, explicit path | test/action.test.js | buildArgs_should_not_emit_config_flag_when_config_input_empty | Unit | Happy path (no auto-discovery) |
| DEC-1: JSON, explicit path | crates/benchdiff/tests/config_cli.rs | cli_should_exit_two_config_file_not_found_when_explicit_path_missing | Integration | Missing explicit path |
| DEC-2: per-metric thresholds | compare.rs | threshold_for_should_prefer_group_name_override_over_default_when_net_parse | Unit | Happy path |
| DEC-2: per-metric thresholds | compare.rs | threshold_for_should_prefer_exact_over_group_over_default | Unit | Three-level lookup (B3) |
| DEC-2: per-metric thresholds | compare.rs | compare_should_apply_group_threshold_when_only_group_key_matches | Unit | `{net: 8, net/Parse: 20}` table |
| DEC-2: per-metric thresholds | matrix.rs | compare_matrix_should_apply_group_threshold_when_only_group_key_matches | Unit | Group key under `group/residual` projection |
| DEC-2: per-metric thresholds | config.rs | parse_config_should_error_when_threshold_and_thresholds_default_both_set | Unit | Error path |
| DEC-2: per-metric thresholds | matrix.rs | compare_matrix_should_apply_group_residual_threshold_override_when_sort_20_percent | Unit | Matrix lookup key |
| DEC-2: per-metric thresholds | crates/benchdiff/tests/config_cli.rs | cli_should_apply_threshold_override_end_to_end_when_config_thresholds | Integration | Config -> kernel |
| DEC-3: axis rules | matrix.rs | project_by_axis_should_split_by_group_when_compare_by_group | Unit | Happy path |
| DEC-3: axis rules | matrix.rs | project_by_axis_should_reject_measure_as_axis_with_message_when_compare_by_measure | Unit | Error path (measure cut) |
| DEC-3: axis rules | matrix.rs | project_by_axis_should_error_rows_collide_naming_both_names_when_group_by_drops_dimension | Unit | Error path |
| DEC-3: axis rules | crates/benchdiff/tests/matrix_cli.rs | cli_should_accept_compare_by_run_equivalent_to_base_head_when_two_columns | Integration | `run` axis |
| DEC-4: standalone SVG only | svg/mod.rs | render_svgs_should_contain_no_external_references_when_standalone | Unit | Happy path |
| DEC-4: standalone SVG only | test/docs.test.js | readme_should_state_svg_is_standalone_artifact_and_no_html_wrapper_when_read | Unit | Doc claim |
| DEC-4: standalone SVG only | test/action.test.js | main_should_write_svg_files_to_workspace_svg_dir_when_publishing_disabled | Integration | No hosting needed for standalone |
| DEC-5: no importer dims | dimensions.rs | extract_defaults_should_not_emit_procs_dim_when_go_importer_output | Unit | Happy path |
| DEC-5: no importer dims | dimensions.rs | extract_defaults_should_leave_name_untouched_when_no_equals_segments_criterion | Unit | Error/edge path |
| DEC-5: no importer dims | crates/benchdiff-core/tests/dimensions_import.rs | importers_should_leave_metric_dims_empty_when_no_extraction_requested | Integration | Golden JSON unchanged |
| DEC-6: config scope | config.rs | parse_config_should_accept_group_key_in_thresholds_and_format_keys_and_reject_markdown_json_output_keys | Unit | Happy: `{net: 8}` and `format`; error: `markdown` is an unknown key |
| DEC-6: config scope | crates/benchdiff/tests/config_cli.rs | cli_should_apply_group_threshold_end_to_end_when_config_thresholds_has_group_key | Integration | Config -> kernel with a bare `group` key |
| DEC-7: failure classes | crates/benchdiff/tests/config_cli.rs | cli_should_exit_two_for_each_static_config_error_class_and_zero_for_each_data_dependent_class | Unit | Table of both classes (supersedes the narrower RC-1 case) |
| DEC-7: failure classes | test/action.test.js | ux_x_2_hosting_and_chart_failures_never_fail_job_data_errors_warn_config_errors_fail | Integration | Three-way matrix |
| DEC-8: embed defaults | test/action.test.js | buildArgs_should_pass_svg_embed_auto_when_input_unset_and_the_given_value_when_set | Unit | `--svg-embed auto` vs explicit |
| DEC-8: embed defaults | test/action.test.js | main_should_embed_when_charts_only_in_config_file_and_svg_embed_input_unset | Integration | `auto` resolved in the binary after layering |
| DEC-9: N-way regression and gate | test/action.test.js | action_variants_should_surface_explicit_reference_error_when_fail_on_regression_without_reference | Integration | Binary's static error fails the step; with `reference: quick` the gate passes/fails per cell and the comment is still posted |
| DEC-9: N-way regression and gate | test/action.test.js | action_outputs_should_count_cells_vs_reference_and_failed_false_without_gate_when_matrix | Integration | Companion `counts`/`failed` for matrix runs |
| FAL-1: falsifier baseline | manual (task 0.3.1b, gate G0, before W6) | MAN-9 baseline half of the falsifier: Q1-Q3 put to at least 3 readers (or a recorded waiver) on `testdata/nway/baseline/pairwise-*.md`, answers and seconds appended to requirements.md "Falsifier baseline result" | Integration | Human observation; if at least 2 of 3 answer all three correctly in under 60 s the plan stops after Phase 1 |
| FAL-1: falsifier baseline | testdata/nway/baseline (task 0.3.1a) | baseline_pairwise_outputs_should_be_byte_identical_on_a_second_run | Unit | Run the unmodified CLI twice and compare; checked in the task, and kept as a regression guard by `nway_fixture.rs` above |
| OM-1: outcome metric | manual (5.1.3a) | MAN-7 time-to-signal walkthrough: three questions on `testdata/nway/expected/matrix.md`, then compare with the baseline recorded at G0 (MAN-9) instead of re-running it; record answers and seconds in ADR-003 | Integration | Human observation; >= 3 readers or a recorded waiver; target < 60 s and all correct for 2 of 3 |
| OM-2: adoption proxy | manual (release+30 and +90 days) | MAN-8 count the owner's repos whose workflows set `variants` or `charts` (grep) and public repos outside (`gh search code`), record both counts in requirements.md | Integration | Human observation; no telemetry; target at least 2 of the owner's repos at +30 days (proposal) |
| SM-4 / C-4 (migration): data format | crates/benchdiff-core/tests/json_compat.rs | migration_should_be_reversible | Migration | Serialize a set with `dims` and a report with `matrix`; strip `dims`/`matrix` (what a schema-1 reader sees); re-parse; assert identity keys and default JSON bytes equal the pre-feature output |

## Domain-term coverage notes
Test signatures use plan Glossary names: `Metric`, `MetricSet`, `Dims`, `Dimensioned`, `Residual`, `VariantLabel`, `Reference`, `Cell`, `Comparison`, `Samples`, `MatrixReport`, `ChartKind`, `SvgFile`, `SkipReason`, `PublishResult`, `OptionsOverlay`.

## UX Acceptance Tests

All 48 criteria from `design/ux.md` are mapped. "Tool": Rust/JS = automated test in the stack; Manual = human-verified checklist (unavoidable for rendering in GitHub and phones).

| UX Criterion | Test File | Test Name | Tool | Steps |
|---|---|---|---|---|
| AC-S1-1 No new options -> byte-identical report | test/golden.test.js | ux_s1_1_default_report_should_equal_expected_when_no_new_options | Rust + JS | Run all 15 CLI golden cases native and wasm plus the Action-level golden, compare to `expected/*` |
| AC-S1-2 Verdict line is first content line (reports that have a verdict) | markdown_matrix.rs | ux_s1_2_verdict_should_be_first_line_after_title_when_matrix_and_two_way | Rust | Render both reports; assert line index 1 after the title is the bold summary, before reference, gate, notice, image or table lines; a notice-only report has no verdict and starts with its first notice |
| AC-S1-3 Alt text <=125 chars + collapsed table | svg/mod.rs | ux_s1_3_alt_should_be_at_most_125_chars_and_details_table_follow_when_chart_embedded | Rust | Build alt for long names; assert the alt names verdict AND top change (`...: 1 regressed, 1 improved; top: BenchmarkSort +30.0%`), length <= 125, name truncated before the verdict or number, equals `<title>`, `<details><summary>Chart data (text)` follows the image with the chart's exact rows, order and `#` numbers (plan 3.3.2; N-way: ratio columns); opened on fallback (see `rewritePlaceholders_should_open_...`) |
| AC-S1-4 Hosting failure line, fallback, warning, job passes | test/action.test.js | ux_s1_4_hosting_failure_should_warn_fallback_and_pass_when_publish_returns_error | JS | Mock publisher `{error, reason:'permission'}`; assert warning line with reason, fix text, the `<svg-dir>` path (not an artifact name) and the `actions/upload-artifact` hint, placeholder stripped, `▰` column present, `core.warning` logged, exit success |
| AC-S1-5 Fork PR read-only token | test/images.test.js | ux_s1_5_fork_pr_should_complete_with_table_and_read_only_warning_when_head_repo_differs | JS | Event payload with differing head/base repo; assert reason, no stack trace, table present |
| AC-S1-6 Missing impl / ref / cap / no samples each produce a line | markdown_matrix.rs | ux_s1_6_degradations_should_each_emit_one_explicit_line_with_counts_and_next_action | Rust | Four fixtures; assert one line each naming count and action (`· 3 incomplete` plus the Method-note sentence naming the impl, `reference "X" absent from 2 benchmarks` with first-10-and-N-more, `+ 17 more not shown`, `no samples in the input`) (plan 2.2.1a, G-10) |
| AC-S1-7 Ref first `(ref)`, `—` for missing, best value bold | markdown_matrix.rs | ux_s1_7_reference_column_should_be_first_labelled_and_best_value_bold_when_matrix | Rust | Snapshot matrix; assert header order, `(ref)`, `—`, bold min |
| AC-S1-8 Ratio phrasing | markdown_matrix.rs | ux_s1_8_ratio_should_print_0_74x_and_use_words_only_at_2x_and_never_higher_lower_or_regress | Rust | Cells at 0.74x, 1.9x, 2.0x, 41x; regex-scan whole report for `higher`/`lower` prose and for `regress` outside the gate line |
| AC-S1-9 Max 8 columns + notice | markdown_matrix.rs | ux_s1_9_columns_should_cap_at_eight_with_json_pointer_when_eleven_variants | Rust | 11 variants; assert 8 columns and `...and 3 more variants not shown (see JSON)` |
| AC-S1-10 Legend directly under the table | markdown_matrix.rs | ux_s1_10_legend_line_should_follow_table_and_define_tilde_when_matrix_has_unchanged_cells | Rust | Assert the line after the table is the legend, that it says bold and Rank are untested orderings (only variant vs reference is tested), that the headline says `has the best geomean` (never `fastest overall`/`loses`), and that the Method note still carries the Holm sentence |
| AC-S1-11 Notice order and cap | markdown_matrix.rs | ux_s1_11_notices_should_follow_fixed_order_cap_at_five_and_collapse_overflow_into_method | Rust | 7 notices of mixed classes; assert order, 5 inline, `⚠️ 2 more notices (see Method)`, all 7 in Method, verdict still first |
| AC-S1-12 N-way regression defined; gate needs explicit reference | crates/benchdiff/tests/matrix_cli.rs | ux_s1_12_nway_gate_should_require_explicit_reference_print_gate_line_and_use_slower_than_reference_wording | Rust | Defaulted reference + `--fail-on-regression` -> exit 2 text; explicit -> gate line and exit 0/1; no flag -> exit 0 |
| AC-S1-13 Compact ratio cells | markdown_matrix.rs | ux_s1_13_cell_format_ratio_should_render_compact_cells_and_keep_reference_absolute | Rust | `ratio` vs `full` snapshots |
| AC-S1-14 Ratio vs percent comprehension | manual (5.1.3a) | MAN-6 show an N-way ratio table and a two-way percent table to >= 3 readers; ask which is slower and by how much; record misreads in ADR-003 | Manual | Two or more misreads reopen the decision (ux.md open items); waiver allowed and recorded |
| AC-S2-1 Same input -> byte-identical SVG, native and WASM | test/golden.test.js | ux_s2_1_svg_should_be_byte_identical_across_runs_and_native_vs_wasm | Rust + JS | Render twice native, once wasm; compare to `expected/svg/*.svg` |
| AC-S2-2 Readable unclipped in light and dark GitHub | manual (G1 / 5.1.3a) | MAN-2 view real PR comment + job summary in GitHub light and dark themes | Manual | Open the sticky comment and summary in both themes on desktop; no clipped text, card visible; record in ADR-003 |
| AC-S2-3 Text contrast >= 4.5:1; series colour never text | svg/style.rs | ux_s2_3_text_contrast_should_be_at_least_4_5_to_1_and_text_fill_never_series_colour | Rust | Compute WCAG contrast from the hex constants (light ink 19.17:1, secondary 7.73:1 on `#fcfcfb`, verified this round; dark pair computed by the test) and assert >= 4.5; scan every `<text>` fill is ink/secondary (plan 3.1.3a, G-7) |
| AC-S2-4 Never colour-only | svg/delta.rs | ux_s2_4_every_bar_should_have_shape_marker_and_value_label | Rust | Parse output; each bar group has marker element and `<text>` value |
| AC-S2-5 Standalone root role=img, title, desc, per-row title; GitHub accessible path is alt + text table | svg/style.rs | ux_s2_5_root_should_have_role_img_aria_labelledby_and_row_titles_with_full_names | Rust | Assert root attributes, ids resolve, one `<title>` per row; docs test asserts the README names alt text and the text table as the accessible path on GitHub |
| AC-S2-6 80-char hostile name truncated, escaped, unclipped | svg/text.rs | ux_s2_6_name_with_80_chars_and_markup_should_truncate_with_ellipsis_escaped_and_fit_gutter | Rust | Name with `<`,`&`,`"`; assert `…`, escaped entities, estimated width <= 212 px |
| AC-S2-7 100 changed benchmarks -> <=30 rows + `+ 70 more not shown` in SVG and Markdown, naming the limiting key; chart never shows a row the table omits | svg/delta.rs | ux_s2_7_hundred_rows_should_show_thirty_and_state_seventy_more_in_svg_and_markdown | Rust | Assert row count 30, footer text in SVG and Markdown line; with `max-rows 20` assert the chart rows are a subset of the table rows; with `sort=name` assert chart order (largest change first) differs from table order and the `#` numbers match the text block |
| AC-S2-8 Legible at 375 px or docs state table primary | manual (5.1.3a) | MAN-3 view comment on phone-width screenshot (375 px) | Manual | Screenshot hosted chart at 375 px; either legible or README sentence present (also asserted by `readme_should_state_table_is_primary_on_phones` in test/docs.test.js) |
| AC-S2-9 Missing = empty slot with `—`, never zero bar | svg/ranking.rs | ux_s2_9_missing_cell_should_draw_hollow_slot_with_dash_and_no_zero_length_bar | Rust | Missing cell fixture; assert hollow marker and no `<rect width="0">` |
| AC-S2-10 Standalone `.svg` opaque, title, desc, no external refs | svg/mod.rs | ux_s2_10_standalone_svg_should_have_opaque_bg_title_desc_and_no_external_references | Rust | Assert background rect first, `<title>`/`<desc>`, no `href`/`url(`/`@import`; open in a browser (manual spot check MAN-4) |
| AC-S2-11 Non-text contrast >= 3:1 by fill or outline | svg/style.rs | ux_s2_11_bars_and_markers_should_have_3_to_1_contrast_by_fill_or_outline_for_light_and_dark | Rust | Compute from the hex constants: delta colours and gray pass as fills; series 3-5 pass through the `#52514e` outline; dark palette computed by the test; fails if the green drops under 3:1 (darken, do not waive) |
| AC-S2-12 Forced-colors and print are a documented limit | svg/style.rs | ux_s2_12_light_card_should_have_no_style_media_or_system_colors_and_readme_should_state_the_limit | Rust + JS | Scan the Light card for `<style>`, `@media`, `currentColor` and system-colour keywords; `test/docs.test.js` asserts the README sentence; print preview spot check in MAN-4 |
| AC-S3-1 No new flags -> same stdout and exit code | crates/benchdiff/tests/golden_expected.rs | ux_s3_1_cli_should_match_previous_stdout_and_exit_code_when_no_new_flags | Rust | Run golden commands, compare stdout and exit codes |
| AC-S3-2 Two-way to charted N-way in <=3 extra flags | crates/benchdiff/tests/matrix_cli.rs | ux_s3_2_variant_charts_svg_dir_should_produce_charted_matrix_with_three_distinct_flags | Rust | Replace base/head with three `--variant`, add `--charts`, `--svg-dir`; assert matrix and files |
| AC-S3-3 `--help` lists flags with defaults; `--print-config` shows sources, exit 0 | crates/benchdiff/tests/config_cli.rs | ux_s3_3_help_should_list_every_new_flag_with_default_and_print_config_should_exit_zero | Rust | Parse help for each new flag name + `default`; run `--print-config`, assert exit 0, no report and the JSON shape with labels `default|config|flag` |
| AC-S3-4 Config errors exit 2 naming key, sample, fix | crates/benchdiff/tests/config_cli.rs | ux_s3_4_config_errors_should_exit_two_and_name_key_value_and_fix | Rust | Table of 8 static errors (enum, unknown key, reference, duplicate label, etc.); assert exit 2 and message parts; the data-dependent rows assert exit 0 with a notice (exit 2 only with `dimensions-strict`) |
| AC-S3-5 Unmatched-dimension message counts + example | dimensions.rs | ux_s3_5_zero_match_message_should_include_matched_total_and_one_example_name | Rust | Assert `matched 0 of 48` and example |
| AC-S3-6 Collision / duplicate label name both sources | crates/benchdiff/tests/matrix_cli.rs | ux_s3_6_collision_and_duplicate_label_should_name_both_sources | Rust | Assert `(a.txt, b.txt)` and both original names |
| AC-S3-7 SVG paths on stderr; stdout report only | crates/benchdiff/tests/svg_cli.rs | ux_s3_7_stdout_should_contain_only_report_and_stderr_list_written_paths | Rust | Capture streams separately |
| AC-S3-8 Unwritable `--svg-dir`: report, then error, exit 2 (CLI only) | crates/benchdiff/tests/svg_cli.rs | ux_s3_8_unwritable_svg_dir_should_print_report_then_error_naming_dir_and_exit_two | Rust | Read-only dir; assert stdout report present, stderr names dir and cause |
| AC-S3-9 Dimensions discoverable | crates/benchdiff/tests/matrix_cli.rs | ux_s3_9_axis_errors_should_end_with_available_dimensions_and_print_dimensions_should_list_detected | Rust | Absent `compare-by`, unknown `group-by`, `compare-by measure` end with `available dimensions: ...`; `--print-dimensions` JSON; `--help` states the zero-config rules |
| AC-S4-1 Input = flag = config key vocabulary (documented exceptions); action.yml states values/defaults | test/action.test.js | ux_s4_1_action_inputs_should_equal_cli_flags_and_config_keys_and_describe_defaults | JS | Parse `action.yml`, `--help` and the config key table; assert set equality for shared names and `default`/allowed values in descriptions |
| AC-S4-2 Precedence demonstrable, Method note names source | crates/benchdiff/tests/config_cli.rs | ux_s4_2_precedence_should_yield_higher_layer_and_method_note_name_source | Rust + JS | Set the same key in file, input and flag; assert the higher layer's value and the source label in the Method note and `--print-config` (decision G-6: an input is a flag to the binary, so the label is `flag`; the test asserts `flag` for both input and flag, `config` for the file) |
| AC-S4-3 Config error fails step with CLI text; data/hosting only warns | test/action.test.js | ux_s4_3_config_error_should_set_failed_with_cli_text_and_data_error_should_only_warn | JS | Bad enum input -> `setFailed` same text; unwritable hosting -> `warning` |
| AC-S4-4 Step log lists produced/skipped; `svg-paths` output | test/action.test.js | ux_s4_4_log_should_list_produced_and_skipped_svgs_and_output_svg_paths | JS | Assert log line and output value |
| AC-S4-5 Hosting side effects disclosed | test/action.test.js | ux_s4_5_hosting_side_effects_should_be_in_action_yml_readme_and_one_log_line | JS | `action.yml` descriptions and README name `contents: write`, branch, per-run commit, retention, public URL, private rule; log line precedes the first write |
| AC-S5-1 Unknown key suggestion; enum lists valid values | config.rs | ux_s5_1_unknown_key_should_suggest_nearest_and_enum_should_list_valid_values | Rust | `chart-row` -> `chart-rows`; `charts: bars` -> valid list |
| AC-S5-2 Parse errors give file, line, column; strict JSON; version required | config.rs | ux_s5_2_parse_error_should_include_file_line_column_and_reject_comments_and_missing_version | Rust | Malformed file with `//` comment; assert `file: line N column M` |
| AC-S5-3 Never auto-discovered; missing explicit path is error | test/action.test.js | ux_s5_3_results_file_named_benchdiff_json_should_change_nothing_and_missing_config_should_error | JS | Workspace with `benchdiff.json`; and `config: nope.json` -> failure |
| AC-S5-4 `charts: delta` alone enables a chart | test/action.test.js | ux_s5_4_charts_delta_alone_should_produce_delta_svg_with_defaults | JS | Single input; assert `delta.svg` produced, 30 rows, width 760, and the image embedded and hosted in comment and summary (Action default) |
| AC-S5-5 Schema and tested starter example | crates/benchdiff/tests/config_schema.rs | ux_s5_5_schema_should_match_key_table_and_starter_example_should_run_and_explain_doubled_backslashes | Rust | Schema equals generated file; example parses, runs, validates; README contains the doubled-backslash note |
| AC-X-1 No dead ends | test/ux_messages.test.js | ux_x_1_every_warning_and_error_message_should_name_an_exit_path | JS + Rust | Collect every message constant from the S1/S3 tables (shared fixture list); assert each contains a fix key, valid values, location or fallback |
| AC-X-2 In the Action, chart and hosting failures never fail the job; config errors always do (CLI scope is AC-S3-8) | test/action.test.js | ux_x_2_chart_failure_should_not_fail_job_and_config_error_should | JS | Matrix of failures: publish error, skip, bad config |
| AC-X-3 Add then remove `charts` restores original comment | test/action.test.js | ux_x_3_removing_charts_should_restore_original_comment_with_marker_and_no_stale_image | JS | Run twice against same sticky comment: with charts, then without; second body equals the golden body, marker preserved, no `raw.githubusercontent` |

UX criteria count check: S1 (14) + S2 (12) + S3 (9) + S4 (5) + S5 (5) + X (3) = 48.

## Test Stack
- **Unit**: Rust `cargo test --workspace` with the standard `#[test]` harness, table-driven tests, a hand-written XML tag-balance checker (no new dev-dependency), `insta`-free snapshot files under `crates/benchdiff-core/tests/snapshots/svg/` with `UPDATE_GOLDEN=1` regeneration; JS `node --test` (`test/*.test.js`) with injected `fetch`, `sleep`, `random` mocks.
- **Integration**: Rust integration tests spawning the `benchdiff` binary (`crates/benchdiff/tests/*.rs`); `test/golden.test.js` (native vs wasm vs committed expected files) with the real `dist/benchdiff.wasm`; `test/action.test.js` running `main()` with mocked `fetch` and a temp workspace; perf via `make perf` (`cargo test --release -- --ignored perf`) and `test/perf.test.js`.
- **E2E / UX**: automated assertions above for 45 of 48 criteria (AC-S2-8 has an automated docs assertion as its alternative branch); manual checklist (G1 / task 0.2.1b and 5.1.3a, recorded in ADR-003) for AC-S2-2, AC-S2-8, AC-S1-14 (MAN-6), the SM-3 viewing step, the OM-1 walkthrough (MAN-7), the FAL-1 falsifier baseline at gate G0 (MAN-9) and the OM-2 adoption proxy (MAN-8); optional Playwright (`ui-playwright`) screenshot of a hosted SVG at 375 px and in light/dark colour schemes if a throwaway page is served locally (does not replace the real-PR check, because Camo and GitHub's sanitizer are what is under test).

## Coverage Targets and How to Measure

| Stack | Coverage command | Target |
|---|---|---|
| Rust (core + CLI) | `cargo tarpaulin --workspace --out Stdout` (or `cargo llvm-cov --workspace --summary-only`) | >= 80% line; >= 90% on `matrix.rs`, `dimensions.rs`, `config.rs`, `svg/*` (new code) |
| JS (Action) | `node --test --experimental-test-coverage test/*.test.js` | >= 80% line on `index.js` and `lib/images.js` |
| WASM | `make golden` (native vs wasm byte equality) | 100% of golden and nway cases |
| Perf | `make perf` | all budgets pass |

- All public service methods (`compare_matrix`, `render_svgs`, `render_markdown_with`, `render_json_with_matrix`, `parse_config`, `apply`, `publishImages`, `rewritePlaceholders`, `buildArgs`, `parseVariants`): happy path + error paths covered above.
- All external integrations (GitHub Git Data API, comment upsert, WASI preopen): unit with mocked `fetch`/stub plus at least one integration test (`publishImages_should_create_orphan_branch_commit...`, `action_variants_should_post_matrix_comment...`, wasm config test in 4.2.3c).
- UX acceptance criteria: each criterion in `design/ux.md` has a corresponding test or manual step (table above).

## Requirements Coverage
34 of 34 requirements have at least one automated or manual test (counted by script against the register); every requirement has a happy and an error-path unit test except where the requirement is a pure guard (C-1, NF-4), which are covered by a positive and a negative guard test, and OM-1, OM-2 and FAL-1, which are manual observations only (MAN-7, MAN-8, MAN-9; FAL-1 also has the byte-identity check of the baseline files). OM-2 and FAL-1 were added in triad round 2. DEC-6, DEC-7 and DEC-8 are the three scope decisions added in round 3 (config scope, failure classes, embed defaults); DEC-9 (N-way regression and gate) and OM-1 (outcome metric, manual) were added in triad round 1; SM-3 is now a per-repo-class matrix and stays partly manual (MAN-1, MAN-5). Migration test `migration_should_be_reversible`: yes (data-format only, no database).

## Gaps Found in plan.md: status after round 3 / phase 4

Each gap was a place where a requirement or UX criterion had a test designed here but the plan had no task/AC that would make it pass. Severity: H = would ship a missing behaviour, M = test exists but plan wording is ambiguous, L = documentation only. All are now closed in plan.md except G-15, which is manual by nature.

| # | Gap | Sev | Status | Resolved in (plan.md) |
|---|---|---|---|---|
| G-1 | No AC for the `Chart data (text)` collapsed block | H | Closed | Story 3.3.2 AC (content, cap line, opened on fallback, absent without chart); `chart_text_table` in 3.3.2a; insertion in 3.3.1c |
| G-2 | Unwritable `--svg-dir` | H | Closed | Story 3.3.1 AC (report first, then error, exit 2; warning only inside the Action) |
| G-3 | AC-X-3 add-then-remove `charts` | M | Closed | Story 4.2.1 AC (two-run sticky comment) |
| G-4 | Config parse errors with file, line, column | M | Closed | Story 1.2.2 AC; `parse_config` takes the file name; the Action stages the file under its base name so the text is identical |
| G-5 | `--help` content | M | Closed | Stories 2.2.2 and 3.3.1 ACs, generated from the key table |
| G-6 | Method-note source provenance (flag vs input) | M | Decided | Labels are `default|config|flag`; an input is a flag to the binary (ADR-001 item 2), so inputs and flags share `flag`; `--print-config` is JSON; ux.md AC-S4-2 and S3 aligned; test `ux_s4_2_...` is written for this option |
| G-7 | Contrast ratio computation | M | Closed | Story 3.1.3 AC (light ink 19.17:1 and secondary 7.73:1 computed in this round; dark pair computed by the test) |
| G-8 | Vocabulary sameness | M | Closed | Story 4.2.1 AC and `crates/benchdiff/tests/vocabulary.rs` with explicit allowlists |
| G-9 | Phone legibility sentence | L | Closed | Story 5.1.1 AC |
| G-10 | Incomplete / reference-absent report lines | M | Closed | Story 2.2.1 ACs (AC-S1-6) |
| G-11 | Fork wording | L | Closed | Story 4.1.1 fixes one error string per reason |
| G-12 | higher/lower prose scan | L | Closed | Story 2.2.1 AC |
| G-13 | I/O-free core, zero-dependency Action | M | Closed | Story 0.1.2 (`purity.rs`) and task 0.1.3a (`package.json`/`require` guard) |
| G-14 | No extra network traffic | L | Closed | Story 4.1.2 AC (host assertion, zero fetch for gated reasons) |
| G-15 | SM-3 and AC-S2-2/S2-8 manual, behind gate G1 | H | Open by design | Support matrix verified class by class in 5.1.3a; never mark SM-3 done from automated tests alone |
| G-16 | Box plot at n >= 20 | L | Closed | Cut from ux.md (strip plot at every n) |
| G-17 | Axis-absent diagnostic at CLI level | L | Closed | Story 2.2.2 ACs (notice and fallback; exit 2 under `dimensions-strict`) |
| G-18 | JSON proof that samples never appear | L | Closed | Test `json_should_exclude_samples_when_keep_samples_true` in the mapping |

## Gaps found by triad review round 1 (product / UX / engineering)

| # | Gap | Status | Resolved in |
|---|---|---|---|
| T-1 | "Regression" undefined for N-way; no gate semantics, Action outputs unclear | Closed | ADR-005 item 7; plan 2.1.3, 2.2.1, 2.2.2, 4.2.1; tests `matrix_failed_should_be_false_without_gate...`, `cli_should_exit_two_when_fail_on_regression_without_explicit_reference_in_nway`, `ux_s1_12_...` |
| T-2 | Notice stacking, order, cap | Closed | plan 2.2.1; test `render_notices_should_order_by_class...`, `ux_s1_11_...` |
| T-3 | Dimension discoverability and zero-config rules | Closed | plan 1.1.2c, 2.1.2, 2.2.2; tests `dimension_summary_should_list_...`, `cli_print_dimensions_...`, `ux_s3_9_...` |
| T-4 | Hosting side effects undisclosed | Closed | plan 4.2.1, 5.1.1; ADR-003; tests `action_yml_should_describe_hosting_side_effects...`, `ux_s4_5_...` |
| T-5 | `chart-rows` vs `max-rows` relationship and "N more" counts | Closed | plan 3.2.4; test `chart_rows_and_max_rows_should_agree...` |
| T-6 | Schema and starter example untested | Closed | plan 5.1.5a; tests in `config_schema.rs` |
| T-7 | Outcome metric and ratio-vs-percent comprehension have no check | Open by design (manual) | MAN-6 and MAN-7 in plan task 5.1.3a; cannot be automated |
| T-8 | Engineering: task 3.3.1a too large, missing Files lines, 2.2.3a in a `then` sequence | Closed | plan tasks 3.3.1a/b/c, Files line on all 54 tasks, own wave W14 |

## Gaps found by triad review round 2 (product READY, UX READY, engineering NEEDS WORK)

| # | Gap | Status | Resolved in |
|---|---|---|---|
| R2-1 | Method-note plumbing (`Summary` fields, `render_method`) had no owning task | Closed | plan 1.2.1b (W2); tests `summary_should_serialize_identically_...`, `render_method_should_list_threshold_overrides_...` |
| R2-2 | Shared types without defining tasks (`Notice`, `CellFormat`, `Correction`, `SvgInput`, `SvgFile`, `SkipReason`, `TextRow`) | Closed | plan 1.1.1c, 2.1.3a, 3.2.4a, 3.3.1a; ownership table |
| R2-3 | W12 signature race on `render_matrix_markdown` | Closed | plan 2.2.1a (frozen signature), 3.3.1c (`_with`), 2.2.2b, 3.3.1b |
| R2-4 | Oversized tasks 1.1.2c, 2.2.1a, 2.2.2a, 4.2.1a, 4.2.3a | Closed | split into 1.1.2c/d, 2.2.1a/b, 2.2.2a/b, 4.2.1a/b, 4.2.3a/b/c |
| R2-5 | `max-rows`, `only-changed`, `title`, `tag` did not reach `compare_matrix` / `render_matrix_markdown` | Closed | plan 2.1.3a (`MatrixSummary`), 2.2.1a; tests `compare_matrix_should_copy_options_...`, `render_matrix_markdown_should_use_summary_...` |
| R2-6 | Purity scan would fail on `#[cfg(test)]` modules; no `package.json`; vocabulary allowlist named inputs that do not exist | Closed | plan 0.1.2a, 0.1.3a, 4.2.1a; tests `core_purity_scan_should_ignore_...`, `package_json_should_be_absent_...`, vocabulary row |
| R2-7 | Chart-dispatch ACs sat in 3.2.4 though `render_svgs` is created in 3.3.1b | Closed | plan 3.3.1 (b) ACs; test `render_svgs_should_skip_with_nothing_to_compare_...` |
| R2-8 | Hosting-failed text named an artifact; no `actions/upload-artifact` hint; first-run permission case undocumented | Closed | ux.md S1/S4, plan 4.1.1, 4.2.1; tests `rewritePlaceholders_should_name_directory_...`, `main_should_post_permission_line_...` |
| R2-9 | N-way `Chart data (text)` fallback content undefined; chart order vs table order uncorrelated; alt text lacked the top change | Closed | plan 3.2.4, 3.3.2; tests `select_rows_should_order_...`, `chart_text_table_should_emit_ratio_columns_...`, `alt_text_should_name_verdict_and_top_change_...` |
| R2-10 | Legend/headline claimed untested head-to-head orderings ("fastest overall", "loses in 0 rows") | Closed | plan 2.2.1 (a); ux.md S1; ADR-005 item 3; tests `render_matrix_markdown_should_say_best_geomean_...`, legend row |
| R2-11 | Non-text contrast 3:1 and forced-colors/print not specified | Closed | ux.md AC-S2-11/12, plan 3.1.3; tests `palette_should_have_3_to_1_...`, `light_card_should_contain_no_style_...` (the delta green measured 3.27:1, so no palette change; light series 3-5 get an outline) |
| R2-12 | Gate FAILED line unbounded when many cells fail | Closed | plan 2.2.1 (b); test `gate_line_should_name_three_cells_...` |
| R2-13 | Product: falsifier threshold wording differed (most readers vs 2 of 3); baseline half scheduled last; no adoption proxy; success metric 1 and Scope said "first/fastest" | Closed | requirements.md; plan 0.3.1a/b and gate G0; tests MAN-9, MAN-8 and `nway_matrix_should_keep_q1_q3_answerable_...` |
| R2-14 | Wasm size headroom risk and re-measure points | Closed (risk stays) | plan 0.1.2a: four re-measure points, levers named |

## Counts

Counted with a script over the mapping table's Type column (UX table counted separately); the script splits each row on ` | ` and counts column 4.

| Type | Count |
|---|---|
| Unit | 136 |
| Integration | 68 (includes the manual observations MAN-1, MAN-5, MAN-7, MAN-8 and MAN-9) |
| Migration | 1 |
| Mapping rows | 205 |
| UX acceptance | 48 (3 manual: AC-S2-2, AC-S2-8, AC-S1-14; rest automated) |
| Total | 253 |
| Requirements covered | 34 / 34 |
| UX criteria mapped | 48 / 48 |
