'use strict';

// Real authoring/export integration; all inputs and outputs stay in synthetic
// temporary projects. Requires the same Emacs/ox-hugo environment as support/check.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {execFileSync} = require('node:child_process');

const repository = path.resolve(__dirname, '..');
const binary = process.env.BERLIN_TEST_BINARY || path.join(repository, 'target/debug/bln');
const project = fs.mkdtempSync(path.join(os.tmpdir(), 'berlin-sqlite-'));
fs.cpSync(path.join(repository, 'support/fixtures/publishing'), project, {recursive: true});
fs.mkdirSync(path.join(project, 'support'), {recursive: true});
fs.cpSync(path.join(repository, 'support/ox-hugo'), path.join(project, 'support/ox-hugo'), {recursive: true});

// Sealing requires destination metadata. This temporary configuration never
// invokes publish and makes no remote requests.
const pipeline = path.join(project, 'berlin.pipeline.rhai');
const original = fs.readFileSync(pipeline, 'utf8');
const assets = '.named("org_assets"),"attachments");';
assert.equal(original.split(assets).length, 2);
fs.writeFileSync(pipeline, original.replace(assets,
    '.named("org_assets"),"attachments").deploy_to(github_pages("example/site"));'));

function run(root, ...args) {
    return execFileSync(binary, args, {
        env: {...process.env, BERLIN_DIR: root, BERLIN_THEME_DIR: path.join(repository, 'themes')},
        encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
    }).trim();
}
function state(root, ...args) { return JSON.parse(run(root, 'state', ...args)); }

state(project, 'migrate');
for (const name of ['org', 'site', 'linkedin']) {
    if (name === 'site') run(project, 'check', '--pipeline', name);
    run(project, 'build', '--pipeline', name);
}
const history = state(project, 'builds').builds;
assert.deepEqual(history.map(build => build.pipeline).sort(), ['linkedin', 'org', 'site']);
assert(history.every(build => build.status === 'succeeded'));
assert(!fs.existsSync(path.join(project, '.berlin/receipts')), 'SQLite builds must not write JSON receipts');
assert(fs.existsSync(path.join(project, '_site/index.html')));
assert(fs.existsSync(path.join(project, '.berlin/linkedin/manifest.json')));
execFileSync(process.execPath, [path.join(repository, 'support/check-attachments.cjs'), project], {stdio: 'pipe'});

const release = run(project, 'release', '--pipeline', 'site');
assert.match(release, /^[0-9a-f]{64}$/);
const status = state(project, 'status');
assert.equal(status.integrity, 'ok');
assert.equal(status.counts.build_runs, 3);
assert.equal(status.counts.sealed_releases, 1);
assert.equal(status.counts.publications, 0, 'local sealing must not create publication evidence');
for (const table of ['metric_definitions', 'metric_publications', 'metric_observations']) {
    assert.equal(status.counts[table], 0, 'acceptance must not seed metrics');
}
const documents = state(project, 'documents');
assert(documents.total > 0);
for (const document of documents.documents) {
    const inspected = state(project, 'document', document.id);
    assert.equal(inspected.metrics.status, 'no_observations');
    assert.equal(inspected.metrics.last_observed_at_unix_seconds, null);
}

const backup = path.join(project, 'lifecycle.sqlite');
state(project, 'backup', backup);
const restored = fs.mkdtempSync(path.join(os.tmpdir(), 'berlin-sqlite-restored-'));
state(restored, 'restore', backup);
assert.deepEqual(state(restored, 'status'), status);
assert.deepEqual(state(restored, 'builds'), state(project, 'builds'));
assert.deepEqual(state(restored, 'documents'), documents);
assert.deepEqual(state(restored, 'metrics'), state(project, 'metrics'));
console.log(`SQLite acceptance passed; synthetic projects retained at ${project} and ${restored}`);
