'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {execFileSync, spawnSync} = require('node:child_process');
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
console.log(`Unified catalog acceptance passed; synthetic project retained at ${project}`);
