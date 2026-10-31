'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {execFileSync, spawnSync} = require('node:child_process');
const {createHash}=require('node:crypto');
const repository = path.resolve(__dirname, '..');
const binary = process.env.BERLIN_TEST_BINARY || path.join(repository, 'target/debug/bln');
const project = fs.mkdtempSync(path.join(os.tmpdir(), 'berlin-unified-resources-'));
fs.cpSync(path.join(__dirname, 'fixtures/unified-resources'), project, {recursive:true});
const env = {...process.env, BERLIN_DIR:project, BERLIN_THEME_DIR:path.join(repository,'themes')};
function run(...args) {return execFileSync(binary,args,{env,encoding:'utf8',stdio:['ignore','pipe','pipe']});}
function snapshot(directory) {
  const files={};
  function visit(dir) {for (const entry of fs.readdirSync(dir,{withFileTypes:true})) {
    const file=path.join(dir,entry.name);
    if(entry.isDirectory()) visit(file); else files[path.relative(directory,file)]=fs.readFileSync(file).toString('base64');
  }}
  visit(directory);return files;
}
function json(file) {return JSON.parse(fs.readFileSync(path.join(project,file),'utf8'));}
const pristine=snapshot(project);
for (const pipeline of ['portfolio','text','site','excerpts','excerpt_text']) {
  const plan=JSON.parse(run('plan','--pipeline',pipeline,'--json'));
  assert(!JSON.stringify(plan).includes('assemble_website'));
  const report=JSON.parse(run('check','--pipeline',pipeline,'--json'));
  assert.equal(report.report.kind,'publication');
  run('build','--pipeline',pipeline,'--dry-run');
}
assert.deepEqual(snapshot(project),pristine,'read-only commands changed the project');
for (const pipeline of ['portfolio','text','site','excerpts','excerpt_text']) run('build','--pipeline',pipeline);
const first=snapshot(path.join(project,'_site'));
run('build','--pipeline','site');
assert.deepEqual(snapshot(path.join(project,'_site')),first,'site rebuild is nondeterministic');
// Invalid ingestion must fail before replacing any installed publication.
const resourceFile=path.join(project,'data/resources.yaml');
const resourceBytes=fs.readFileSync(resourceFile);
fs.writeFileSync(resourceFile,'schema_version: 1\nresources:\n  - id: incomplete\n');
const badResource=spawnSync(binary,['build','--pipeline','site'],{env,encoding:'utf8'});
assert.notEqual(badResource.status,0);
assert.match(badResource.stderr,/InvalidResource/);
assert.deepEqual(snapshot(path.join(project,'_site')),first,'parse failure installed partial resources');
fs.writeFileSync(resourceFile,resourceBytes);

// Explicit helper bytes are consumed even when a change leaves composition alone.
const helper=path.join(project,'qualified-helper.rhai');
fs.writeFileSync(helper,'const QUALIFICATION_HELPER = 1;\n');
const selectedFiles=['--pipeline-file',helper,'--pipeline-file',path.join(project,'berlin.pipeline.rhai')];
run('build','--pipeline','portfolio',...selectedFiles);
const firstHelper=json('_portfolio/publication.json');
fs.writeFileSync(helper,'const QUALIFICATION_HELPER = 2;\n');
run('build','--pipeline','portfolio',...selectedFiles);
const secondHelper=json('_portfolio/publication.json');
assert.equal(firstHelper.composition_fingerprint,secondHelper.composition_fingerprint);
assert.notEqual(firstHelper.render_input_fingerprint,secondHelper.render_input_fingerprint);
assert.equal(json('.berlin/receipts/portfolio.json').inputs.find(input=>input.path==='qualified-helper.rhai').sha256,
  createHash('sha256').update(fs.readFileSync(helper)).digest('hex'));
fs.rmSync(helper);
run('build','--pipeline','portfolio');
// Only selected stylesheet bytes enter the operational fingerprint. An unused
// override cannot break or alter a publication; a selected invalid import must
// fail equally in check, dry-run and the real transaction.
const styleDirectory=path.join(project,'styles/publications');
fs.mkdirSync(styleDirectory,{recursive:true});
const unusedStyle=path.join(styleDirectory,'unused.css');
fs.writeFileSync(unusedStyle,"@import 'missing.css';");
run('build','--pipeline','site');
assert.deepEqual(snapshot(path.join(project,'_site')),first,'unused CSS altered the output');
const activeStyle=path.join(styleDirectory,'portfolio.css');
fs.writeFileSync(activeStyle,"@import 'missing.css';");
const beforeInvalidCss=snapshot(project);
for(const args of [['check','--pipeline','site','--json'],['build','--pipeline','site','--dry-run'],['build','--pipeline','site']]) {
  const result=spawnSync(binary,args,{env,encoding:'utf8'});
  assert.notEqual(result.status,0,`${args.join(' ')} accepted a missing CSS import`);
  assert.match(result.stdout+result.stderr,/missing captured stylesheet/);
  assert.deepEqual(snapshot(path.join(project,'_site')),first,'invalid CSS replaced installed output');
}
// Real builds may record a failed run outside the output; read-only commands
// must not change any project byte.
for(const args of [['check','--pipeline','site','--json'],['build','--pipeline','site','--dry-run']]) {
  const before=snapshot(project);
  spawnSync(binary,args,{env,encoding:'utf8'});
  assert.deepEqual(snapshot(project),before,'read-only CSS validation changed the project');
}
assert(beforeInvalidCss['styles/publications/portfolio.css']);
fs.rmSync(activeStyle);fs.rmSync(unusedStyle);
const notePath=path.join(project,'content/storage-latency.md');
const originalNote=fs.readFileSync(notePath,'utf8');
fs.writeFileSync(notePath,originalNote+'\n[Missing target](id:absent-target)\n');
const beforeBadLink=snapshot(project);
const badLink=spawnSync(binary,['check','--pipeline','site','--json'],{env,encoding:'utf8'});
assert.notEqual(badLink.status,0);
const badLinkReport=JSON.parse(badLink.stdout);
assert(badLinkReport.report.findings.some(finding=>finding.code==='unresolved_reference'&&finding.target==='absent-target'));
assert.deepEqual(snapshot(project),beforeBadLink,'invalid-link check changed the project');
fs.writeFileSync(notePath,originalNote);
assert.equal(json('_portfolio/publication.json').composition_fingerprint,json('_text/manifest.json').publication.composition_fingerprint);
assert.equal(json('_excerpt_text/manifest.json').profile,'linkedin_draft');
for (const file of ['index.html','notes.html','about.html','feed.html','notes/storage-latency.html','notes/recovery-design.html','tags/storage.html','search/index.json','search/client.js','notes/old-storage-latency.html','attachments/handoff.svg','attachments/format.txt']) assert(fs.existsSync(path.join(project,'_site',file)),`missing ${file}`);
const article=fs.readFileSync(path.join(project,'_site/notes/storage-latency.html'),'utf8');
for(const text of ['handoff-line-1','Figure','Referenced in']) {
  if(text==='Figure') assert(article.includes('A synthetic handoff diagram'));
  else assert(article.includes(text),`article missing ${text}`);
}
const search=json('_site/search/index.json');
assert(search.documents.some(resource=>resource.path==='https://github.com/tigerbeetle/tigerbeetle'));
assert(!Object.keys(first).some(file=>file.includes('tigerbeetle.html')));
for (const output of ['_site','_portfolio','_text','_excerpts','_excerpt_text']) {
  for(const [name,bytes] of Object.entries(snapshot(path.join(project,output)))) {
    if(!/\.(json|html|txt)$/.test(name)) continue;
    const text=Buffer.from(bytes,'base64').toString('utf8');
    for(const sentinel of [project,'DRAFT_BODY_SENTINEL','DRAFT_TITLE_SENTINEL','DRAFT_DESCRIPTION_SENTINEL']) assert(!text.includes(sentinel),`${output}/${name} leaks ${sentinel}`);
    if(output!=='_site') assert(!text.includes('UNSELECTED_BODY_SENTINEL'),`${output}/${name} leaked unselected body`);
  }
}
const excerpts=fs.readFileSync(path.join(project,'_excerpts/index.html'),'utf8');
assert.equal((excerpts.match(/id="occ-[^"]*--prefix-line-1"/g)||[]).length,2);
assert.equal((excerpts.match(/id="occ-[^"]*--fn-prefix"/g)||[]).length,2);
const script=path.join(project,'berlin.pipeline.rhai');
const original=fs.readFileSync(script,'utf8');
fs.writeFileSync(script, original.replace('"about","about.html"','"about","INDEX.html"'));
const collision=spawnSync(binary,['build','--pipeline','site'],{env,encoding:'utf8'});
assert.notEqual(collision.status,0);
assert.match(collision.stderr,/duplicate site route/);
assert.deepEqual(snapshot(path.join(project,'_site')),first,'route conflict replaced installed output');
fs.writeFileSync(script,original+`\n
pipeline root_site {
  output render_html(compose(sources(),Fn("notebook")),"_root_site",html_config(#{theme:"@notebook",url:"https://example.test",title:"Root notebook",author:"Example Author"}))
    .with_assets(load_assets("attachments/**/*"),"attachments");
}
pipeline invalid_site_text {
  output render_text(compose(sources(),Fn("notebook")),"_invalid_text",text_config(#{}));
}
`);
run('build','--pipeline','root_site');
const redirect=fs.readFileSync(path.join(project,'_root_site/notes/old-storage-latency.html'),'utf8').replaceAll('&#x2F;','/');
assert(redirect.includes('https://example.test/notes/storage-latency.html'));
const textSite=spawnSync(binary,['build','--pipeline','invalid_site_text'],{env,encoding:'utf8'});
assert.notEqual(textSite.status,0);
assert.match(textSite.stderr,/UnsupportedChannelFeature/);
assert(!fs.existsSync(path.join(project,'_invalid_text')));
fs.writeFileSync(script,original);
function verifyInventory(directory,manifestFile,text=false) {
  const full=JSON.parse(fs.readFileSync(path.join(directory,manifestFile),'utf8'));
  const manifest=text?full.publication:full;
  assert.equal(manifest.output.policy,'all-files-except-own-manifest/v1');
  const actual=snapshot(directory);delete actual[manifestFile];
  assert.deepEqual(manifest.output.files.map(file=>file.path),Object.keys(actual).sort());
  for(const file of manifest.output.files) {
    const bytes=Buffer.from(actual[file.path],'base64');
    assert.equal(file.bytes,bytes.length);
    assert.equal(file.sha256,createHash('sha256').update(bytes).digest('hex'));
  }
  return manifest;
}
for(const output of ['_site','_portfolio','_excerpts']) verifyInventory(path.join(project,output),'publication.json');
for(const output of ['_text','_excerpt_text']) verifyInventory(path.join(project,output),'manifest.json',true);
const portfolio=json('_portfolio/publication.json');
assert.equal(Object.keys(portfolio.queries).length,2);
assert(portfolio.catalog_inputs.some(input=>input.logical_name==='data/resources.yaml'));
for(const [key,evidence] of Object.entries(portfolio.queries)) {
  assert.equal(key,evidence.query_digest);
  assert(evidence.selected.every(entry=>/^(local|external):/.test(entry.id)&&/^[a-f0-9]{64}$/.test(entry.revision)));
}
const presentation=path.join(project,'pages/components/berlin/resource_card.v1.tera');
fs.mkdirSync(path.dirname(presentation),{recursive:true});
fs.writeFileSync(presentation,fs.readFileSync(path.join(repository,'themes/notebook/shared/pages/components/berlin/resource_card.v1.tera'),'utf8').replace('publication-resource','publication-resource alternate-resource'));
run('check','--pipeline','portfolio');run('build','--pipeline','portfolio');
const alternate=json('_portfolio/publication.json');
assert.equal(alternate.composition_fingerprint,portfolio.composition_fingerprint);
assert.notEqual(alternate.render_input_fingerprint,portfolio.render_input_fingerprint);
assert(fs.readFileSync(path.join(project,'_portfolio/index.html'),'utf8').includes('alternate-resource'));
fs.rmSync(presentation);run('build','--pipeline','portfolio');
assert.equal(json('_portfolio/publication.json').render_input_fingerprint,portfolio.render_input_fingerprint);
const relocated=fs.mkdtempSync(path.join(os.tmpdir(),'berlin-unified-relocated-'));
fs.cpSync(path.join(__dirname,'fixtures/unified-resources'),relocated,{recursive:true});
execFileSync(binary,['build','--pipeline','site'],{env:{...env,BERLIN_DIR:relocated},stdio:'pipe'});
assert.deepEqual(snapshot(path.join(relocated,'_site')),first,'project relocation altered public bytes');
// Local sealing exercises the normal release lifecycle without any publishing.
fs.writeFileSync(script,original.replace('.with_assets(load_assets("attachments/**/*"),"attachments");','.with_assets(load_assets("attachments/**/*"),"attachments").deploy_to(github_pages("example/notebook"));'));
const release=run('release','--pipeline','site','--production').trim();
assert.match(release,/^[a-f0-9]{64}$/);
const bundle=path.join(project,'.berlin/releases',release);
const sealed=JSON.parse(fs.readFileSync(path.join(bundle,'manifest.json'),'utf8'));
assert.deepEqual(sealed.documents.entries.map(entry=>entry.id),['footnotes-example','recovery-design','rust-futures','storage-latency']);
const production=verifyInventory(path.join(bundle,'site'),'publication.json');
assert.equal(production.output.processing,'berlin-production/v1');
assert.notEqual(production.render_input_fingerprint,json('_site/publication.json').render_input_fingerprint);
assert(sealed.files.some(file=>file.path==='publication.json'));
run('release-diff',release,release,'--json');
const sealedBefore=snapshot(bundle);
const imported=run('release','--pipeline','site','--from-directory','_site').trim();
assert(!('documents' in json(`.berlin/releases/${imported}/manifest.json`)),'imported bytes gained document provenance');
run('release-diff',release,imported,'--json');
assert.deepEqual(snapshot(bundle),sealedBefore,'later operations rewrote an existing sealed release');
assert(!fs.existsSync(path.join(project,'.berlin/publications')),'local release created a publication ledger');
fs.writeFileSync(script,original);
console.log(`Unified catalog acceptance passed; synthetic project retained at ${project}`);
