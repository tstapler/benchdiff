'use strict';
// README coverage checks: every Action input and CLI flag is documented, the
// example config parses, links resolve, and the zero-config rules match --help.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const root = path.join(__dirname, '..');
const readme = fs.readFileSync(path.join(root, 'README.md'), 'utf8');
const actionYml = fs.readFileSync(path.join(root, 'action.yml'), 'utf8');
const bin = process.env.BENCHDIFF_NATIVE || path.join(root, 'target/release/benchdiff');
const squash = (s) => s.replace(/\s+/g, ' ').trim();

function help() {
  const r = spawnSync(bin, ['--help'], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  return r.stdout;
}

function actionInputs() {
  const block = actionYml.split(/^inputs:\n/m)[1].split(/^outputs:\n/m)[0];
  return [...block.matchAll(/^ {2}([a-z][a-z-]*):\s*$/gm)].map((m) => m[1]);
}

function actionOutputs() {
  const block = actionYml.split(/^outputs:\n/m)[1].split(/^runs:\n/m)[0];
  return [...block.matchAll(/^ {2}([a-z][a-z-]*):\s*$/gm)].map((m) => m[1]);
}

test('README mentions every Action input and output', () => {
  const inputs = actionInputs();
  assert.ok(inputs.length >= 30, `parsed ${inputs.length} inputs`);
  for (const name of [...inputs, ...actionOutputs()]) {
    assert.ok(readme.includes('`' + name + '`'), `README lacks \`${name}\``);
  }
});

test('README mentions every CLI flag in --help', () => {
  const flags = new Set(help().match(/--[a-z][a-z-]*/g));
  assert.ok(flags.size >= 30, `parsed ${flags.size} flags`);
  for (const flag of flags) {
    assert.ok(readme.includes(flag), `README lacks ${flag}`);
  }
});

test('zero-config dimension rules equal the --help text', () => {
  const m = squash(help()).match(/zero-config rules read only .*? carry no '='\)/);
  assert.ok(m, 'rules sentence not found in --help');
  assert.ok(squash(readme).includes(m[0]), 'README rules differ from --help');
});

test('README code blocks naming the example config parse', () => {
  const example = fs.readFileSync(path.join(root, 'examples/benchdiff.config.json'), 'utf8');
  const cfg = JSON.parse(example);
  assert.equal(cfg.version, 1);
  assert.match(readme, /examples\/benchdiff\.config\.json/);
  assert.match(readme, /benchdiff\.config\.schema\.json/);
  const schema = JSON.parse(fs.readFileSync(path.join(root, 'benchdiff.config.schema.json'), 'utf8'));
  for (const key of Object.keys(cfg).filter((k) => k !== '$schema')) {
    assert.ok(key in schema.properties, `schema lacks ${key}`);
  }
  // Any ```json block that is a whole config (has "version") must parse.
  for (const m of readme.matchAll(/```json\n([\s\S]*?)```/g)) {
    JSON.parse(m[1]);
  }
});

test('relative links resolve', () => {
  const links = [...readme.matchAll(/\]\(([^)\s]+)\)/g)].map((m) => m[1]);
  const headings = new Set(
    [...readme.matchAll(/^#{1,6} (.+)$/gm)].map((m) =>
      m[1].toLowerCase().replace(/[^\w\s-]/g, '').trim().replace(/\s+/g, '-'),
    ),
  );
  for (const link of links) {
    if (/^https?:/.test(link)) continue;
    if (link.startsWith('#')) {
      assert.ok(headings.has(link.slice(1)), `no heading for ${link}`);
    } else {
      assert.ok(fs.existsSync(path.join(root, link.split('#')[0])), `missing file ${link}`);
    }
  }
});

test('Hosting images discloses every side effect', () => {
  const sec = readme.split(/^## Hosting images$/m)[1].split(/^## /m)[0];
  for (const needle of [
    'permissions: contents: write',
    'benchdiff-assets',
    'one commit per run',
    'never pruned',
    'Anyone with the URL',
    'Private repositories are never hosted',
    'Fork PRs',
    'UNVERIFIED',
  ]) {
    assert.ok(sec.includes(needle), `Hosting images lacks "${needle}"`);
  }
});

test('N-way, config and gate semantics are stated', () => {
  for (const needle of [
    'never auto-discovered',
    '--no-only-changed',
    '--no-fail-on-regression',
    'has no negative form',
    'Set `reference` explicitly',
    'Holm',
    'geometric mean',
    '`0.74x` means a value 26% lower',
    'requires an explicit `reference`',
    'no telemetry',
    '--print-dimensions',
    '`group` is a pseudo-dimension',
    'defaults < config file < flags',
  ]) {
    assert.ok(readme.includes(needle), `README lacks "${needle}"`);
  }
});

test('matrix threshold-key and --base notices are documented', () => {
  for (const needle of [
    "threshold key '...' matches no benchmark (matrix keys are group/residual or group)",
    '`--base` given with `--compare-by` is ignored by the matrix',
    'summarizes the union of all variant files',
  ]) {
    assert.ok(readme.includes(needle), `README lacks "${needle}"`);
  }
});

test('concept map, source labels, notice rules and gate-line rules are documented', () => {
  const sec = readme.split(/^### Concept map$/m)[1].split(/^## /m)[0];
  assert.match(sec, /\| Concept \| Action input \| CLI flag \| Config key \|/);
  for (const needle of [
    '`default|config|flag`',
    '`threshold=3 (config)`',
    'fixed class order: degradation, skipped metrics, reference-absent, invalid cells, chart notices',
    'The first 5 are inline',
    '`N more notices (see Method)`',
    'the Method note lists every notice',
    'names up to 3 of them in table order',
    'cut to 40 characters',
    'capped at 200 characters',
    'the Method note has the full list',
  ]) {
    assert.ok(squash(sec).includes(needle), `README lacks "${needle}"`);
  }
  // Every config key is on a concept-map row.
  const schema = JSON.parse(fs.readFileSync(path.join(root, 'benchdiff.config.schema.json'), 'utf8'));
  for (const key of Object.keys(schema.properties).filter((k) => !['$schema', 'version'].includes(k))) {
    assert.ok(sec.includes('`' + key + '`'), `concept map lacks \`${key}\``);
  }
});

test('README documents the sanitised axis-value labels', () => {
  assert.ok(readme.includes('shown with disallowed characters replaced by `_`'), 'README lacks the label sanitising sentence');
});

const chartsSection = readme.split(/^## Charts$/m)[1].split(/^## /m)[0];
const hostingSection = readme.split(/^## Hosting images$/m)[1].split(/^## /m)[0];

test('readme_should_state_svg_is_standalone_artifact_and_no_html_wrapper_when_read', () => {
  for (const needle of ['standalone artifact', 'no HTML wrapper', 'actions/upload-artifact']) {
    assert.ok(chartsSection.includes(needle), `Charts lacks "${needle}"`);
  }
});

test('readme_should_state_table_is_primary_on_phones', () => {
  assert.ok(chartsSection.includes('table is the primary view on phones'));
});

test('readme_should_document_chart_scope_accessibility_contrast_and_first_run_permissions', () => {
  const needles = [
    [readme, 'across that row only'], // per-row Holm scope
    [readme, '`svg-embed`'],
    [hostingSection, 'Support by repository class'], // the support matrix
    [chartsSection, 'alt text'],
    [chartsSection, 'Chart data (text)'],
    [chartsSection, 'Forced-colors and print'],
    [chartsSection, '3:1'],
    [hostingSection, 'permissions:'],
    [hostingSection, 'contents: write'],
    [hostingSection, 'pull-requests: write'],
    [hostingSection, 'First run: with a read-only default `GITHUB_TOKEN`'],
  ];
  for (const [text, needle] of needles) {
    assert.ok(text.includes(needle), `README lacks "${needle}"`);
  }
});

test('readme_should_note_regex_backslashes_are_doubled_in_json_examples', () => {
  assert.ok(squash(readme).includes('Regex backslashes are doubled in the JSON examples'));
  const example = fs.readFileSync(path.join(root, 'examples/benchdiff.config.json'), 'utf8');
  assert.ok(example.includes('\\\\d'), 'the example config really doubles the backslash');
});
