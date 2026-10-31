'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {execFileSync, spawnSync} = require('node:child_process');
const repository = path.resolve(__dirname, '..');
const binary = process.env.BERLIN_TEST_BINARY || path.join(repository, 'target/debug/bln');
const project = fs.mkdtempSync(path.join(os.tmpdir(), 'berlin-publications-acceptance-'));
fs.cpSync(path.join(__dirname, 'fixtures/composable-publications'), project, {recursive:true});
const env = {...process.env, BERLIN_DIR:project, BERLIN_THEME_DIR:path.join(repository,'themes')};
function run(...args) {return execFileSync(binary,args,{env,encoding:'utf8',stdio:['ignore','pipe','pipe']});}
function snapshot(dir) {
  const files={};
  function visit(directory) {for (const entry of fs.readdirSync(directory,{withFileTypes:true})) {
    const file=path.join(directory,entry.name);
    if(entry.isDirectory()) visit(file); else files[path.relative(dir,file)]=fs.readFileSync(file).toString('base64');
  }}
  visit(dir);return files;
}
function json(file) {return JSON.parse(fs.readFileSync(path.join(project,file),'utf8'));}
const authored=snapshot(path.join(project,'content'));
const pristine=snapshot(project);
for (const pipeline of ['portfolio','curated','excerpts','text','excerpt_text']) {
  run('plan','--pipeline',pipeline,'--json');
  assert.equal(JSON.parse(run('check','--pipeline',pipeline,'--json')).report.kind,'publication');
  run('build','--pipeline',pipeline,'--dry-run');
}
assert.deepEqual(snapshot(project),pristine,'planning/checking/dry-run changed the project');
for (const pipeline of ['portfolio','curated','excerpts','text','excerpt_text']) run('build','--pipeline',pipeline);
const first=snapshot(path.join(project,'_portfolio'));
run('build','--pipeline','portfolio');assert.deepEqual(snapshot(path.join(project,'_portfolio')),first);
const html=json('_portfolio/publication.json');
assert.equal(html.composition_fingerprint,json('_text/manifest.json').publication.composition_fingerprint);
assert.equal(json('_excerpt_text/manifest.json').profile,'linkedin_draft');
const excerpt=fs.readFileSync(path.join(project,'_excerpts/index.html'),'utf8');
assert.equal((excerpt.match(/<h1\b/g)||[]).length,1);
assert(excerpt.includes('handoff-line-1'),'code-line references missing');
assert(!excerpt.includes('id="handoff-line-1"'),'line reference escaped occurrence namespace');
for(const output of ['_portfolio','_excerpts','_text','_excerpt_text']) {
  for(const [name,bytes] of Object.entries(snapshot(path.join(project,output)))) {
    if(!/\.(json|html|txt)$/.test(name)) continue;
    const text=Buffer.from(bytes,'base64').toString('utf8');
    for(const privateText of [project,'DRAFT_BODY_SENTINEL','UNSELECTED_BODY_SENTINEL']) assert(!text.includes(privateText),`${output}/${name} leaks ${privateText}`);
  }
}
const script=path.join(project,'berlin.pipeline.rhai');const original=fs.readFileSync(script,'utf8');
const invalid=JSON.parse(fs.readFileSync(path.join(project,'invalid/cases.json'),'utf8'));
for(const test of invalid) {
  const file=test.source ? path.join(project,test.source) : script;
  const bytes=fs.readFileSync(file,'utf8');assert(bytes.includes(test.from),`stale invalid fixture ${test.name}`);
  fs.writeFileSync(file,bytes.replaceAll(test.from,test.to));
  const output=spawnSync(binary,['build','--pipeline',test.pipeline],{env,encoding:'utf8'});
  assert.notEqual(output.status,0,`${test.name} unexpectedly built`);
  fs.writeFileSync(file,bytes);
  assert.deepEqual(snapshot(path.join(project,'_portfolio')),first,'failed build replaced output');
}
fs.cpSync(path.join(project,'alternate/pages'),path.join(project,'pages'),{recursive:true});
fs.cpSync(path.join(project,'alternate/styles'),path.join(project,'styles'),{recursive:true});
run('check','--pipeline','portfolio');run('build','--pipeline','portfolio');
const alternate=json('_portfolio/publication.json');
assert.equal(html.composition_fingerprint,alternate.composition_fingerprint);
assert.notEqual(html.render_input_fingerprint,alternate.render_input_fingerprint);
assert(fs.readFileSync(path.join(project,'_portfolio/index.html'),'utf8').includes('alternate-work'));
fs.writeFileSync(script,original.replace('author: "Example Author"}));','author: "Example Author"})).deploy_to(github_pages("example/notebook"));'));
const release=run('release','--pipeline','portfolio').trim();
assert.match(release,/^[a-f0-9]{64}$/);
const sealed=json(`.berlin/releases/${release}/manifest.json`);
assert(sealed.files.some(file=>file.path==='publication.json'));
assert(!('documents' in sealed));
// Existing comparison verifies both complete local bundles without remote access.
run('release-diff',release,release,'--json');
assert(!fs.existsSync(path.join(project,'.berlin/publications')));
assert.deepEqual(snapshot(path.join(project,'content')),authored);
console.log(`Publication acceptance passed; synthetic artifacts retained at ${project}`);
