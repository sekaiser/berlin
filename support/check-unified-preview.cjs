'use strict';
// HTTP-only watcher qualification. Does not open or automate a browser.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const http=require('node:http');
const {spawn}=require('node:child_process');
const repository=path.resolve(__dirname,'..');
const binary=process.env.BERLIN_TEST_BINARY||path.join(repository,'target/debug/bln');
const project=fs.mkdtempSync(path.join(os.tmpdir(),'berlin-unified-preview-'));
fs.cpSync(path.join(__dirname,'fixtures/unified-resources'),project,{recursive:true});
const port=30000+process.pid%20000;
const logPath=path.join(os.tmpdir(),`berlin-unified-preview-${process.pid}.log`);
const log=fs.openSync(logPath,'w');
const child=spawn(binary,['serve','--pipeline','site','--watch','--port',String(port)],{
  env:{...process.env,BERLIN_DIR:project,BERLIN_THEME_DIR:path.join(repository,'themes')},
  stdio:['ignore',log,log],
});
const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
function get(route) {return new Promise((resolve,reject)=>{
  const request=http.get({host:'127.0.0.1',port,path:route,timeout:1500},response=>{
    let body='';response.setEncoding('utf8');response.on('data',chunk=>body+=chunk);
    response.on('end',()=>response.statusCode===200?resolve(body):reject(new Error(`GET ${route}: ${response.statusCode}`)));
  });request.on('error',reject);request.on('timeout',()=>request.destroy(new Error('GET timeout')));
});}
async function until(test,label) {
  const end=Date.now()+30000;
  while(Date.now()<end) {
    if(child.exitCode!==null) throw new Error(`preview exited ${child.exitCode}: ${fs.readFileSync(logPath,'utf8')}`);
    try {const value=await test();if(value) return value;} catch {}
    await pause(200);
  }
  throw new Error(`Timed out: ${label}\n${fs.readFileSync(logPath,'utf8')}`);
}
async function revision() {return (await get('/__berlin/revision')).trim();}
async function changed(previous,label) {return until(async()=>{const next=await revision();return next!==previous&&next;},label);}
(async()=>{
  try {
    await until(async()=> (await get('/')).includes('/__berlin/live.js'),'initial page');
    await until(async()=>fs.readFileSync(logPath,'utf8').includes('Watching for changes'),'initial build completion');
    let rev=await revision();
    const resources=path.join(project,'data/resources.yaml');
    fs.appendFileSync(resources,"\n  - id: preview-created\n    kind: website\n    url: https://example.test/preview-created\n    title: Preview created resource\n    topics: [storage]\n");
    rev=await changed(rev,'resource edit');
    assert((await get('/search/index.json')).includes('Preview created resource'));
    const override=path.join(project,'styles/publications/portfolio.css');
    fs.mkdirSync(path.dirname(override),{recursive:true});
    fs.writeFileSync(override,'.preview-created { color: rebeccapurple; }');
    rev=await changed(rev,'CSS override creation');
    assert((await get('/assets/css/portfolio.css')).includes('.preview-created'));
    fs.rmSync(override);
    rev=await changed(rev,'CSS override removal');
    assert(!(await get('/assets/css/portfolio.css')).includes('.preview-created'));
    const note=path.join(project,'content/storage-latency.md');
    fs.appendFileSync(note,'\nPreview source edit sentinel.\n');
    rev=await changed(rev,'document edit');
    assert((await get('/notes/storage-latency.html')).includes('Preview source edit sentinel'));
    // Wait for any coalesced input notification before testing output isolation.
    await pause(2500);rev=await revision();
    fs.writeFileSync(path.join(project,'_site/preview-output-only.txt'),'output write');
    await pause(3500);
    assert.equal(await revision(),rev,'installed output triggered a rebuild');
    assert(!fs.readFileSync(path.join(project,'_site/index.html'),'utf8').includes('/__berlin/live.js'));
    console.log(`Unified HTTP preview acceptance passed; project ${project}; log ${logPath}`);
  } finally {
    child.kill('SIGINT');
    await Promise.race([new Promise(resolve=>child.once('exit',resolve)),pause(3000)]);
    if(child.exitCode===null) child.kill('SIGKILL');
    fs.closeSync(log);
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
