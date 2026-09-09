// Opt-in browser checks; Playwright is supplied by the caller, not shipped with the theme.
const {chromium} = require(process.env.BERLIN_PLAYWRIGHT_PATH || 'playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {previewSpecimen} = require('./preview.cjs');
const {inspectResults,inspectWideResult} = require('./results-browser.cjs');
const {inspectComparison} = require('./comparisons-browser.cjs');
const {inspectDiagram} = require('./diagrams-browser.cjs');
const {inspectObservations} = require('./observations-browser.cjs');
const {inspectEntries} = require('./entries-browser.cjs');
const {inspectReference} = require('./reference-browser.cjs');
const artifacts = process.env.BERLIN_VALIDATION_DIR || fs.mkdtempSync(path.join(os.tmpdir(), 'berlin-theme-browser-'));

async function inspect(browser, project, size, width, height, javaScriptEnabled, enlarged = false) {
  const context = await browser.newContext({viewport:{width,height}, javaScriptEnabled, reducedMotion:'reduce', acceptDownloads:true});
  const errors = [];
  const base = new URL(project.url + '/');
  try {
    // Prevent off-site requests; the local server supplies the actual build bytes.
    await context.route('**/*', async route => {
      const url = new URL(route.request().url());
      if (route.request().method() !== 'GET' || url.origin !== base.origin || !url.pathname.startsWith(base.pathname)) {
        errors.push(`Unexpected request: ${url}`); return route.abort();
      }
      await route.continue();
    });
    const page = await context.newPage();
    page.setDefaultTimeout(10000);
    page.on('pageerror', error => errors.push(error.message));
    page.on('response', response => {if (response.status() >= 400) errors.push(`${response.status()}: ${response.url()}`);});
    async function visit(route) {
      await page.goto(`${project.url}/${route}`);
      await page.evaluate(()=>document.fonts.ready);
      if (enlarged) await page.addStyleTag({content:'html { font-size: 200% !important; }'});
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth <= innerWidth), `${route}: overflow at ${size}`);
    }
    const screenshot = name => page.screenshot({path:path.join(artifacts, `${name}-${size}-${javaScriptEnabled ? 'js' : 'no-js'}.png`)});
    if (project.variant === 'empty') {
      for (const route of ['', 'notes.html', 'feed.html', 'search.html']) {
        await visit(route);
        assert.equal(await page.locator('.journal-entry,.journal-bookmark').count(),0);
        if(route==='feed.html') {
          assert.equal(await page.locator('.reading-tools').isVisible(),false);
          assert.ok(await page.getByText('No saved references yet.',{exact:true}).isVisible());
          await screenshot('empty-reading');
        }
      }
    } else {
      for(const route of ['', 'notes.html', 'notes/long-title.html', 'notes/untagged.html', 'tags/design.html']) {
        await visit(route);
        if (route === '') await inspectEntries(page,screenshot,width,size==='desktop' && javaScriptEnabled,enlarged);
        else assert.equal(await page.locator('.journal-entry--featured').count(),0);
        if(route==='notes/long-title.html') {
          assert.equal(await page.locator('.study-topics .paper-tag').count(),20);
          const overlap = await page.evaluate(()=>{
            const title=document.querySelector('h1').getBoundingClientRect();
            const tags=document.querySelector('.study-topics').getBoundingClientRect();
            return title.bottom > tags.top;
          });
          assert.equal(overlap,false);
          await screenshot('long-title');
          if(width<=600) await inspectWideResult(page,screenshot);
        }
        if(route==='notes/untagged.html') {
          assert.equal(await page.locator('.study-topics').count(),0);
          assert.equal(await page.locator('.study-contents').isVisible(),false);
        }
      }
      await visit('notes/component-specimens.html');
      await inspectReference(page,screenshot,width,javaScriptEnabled);
      const listing=page.locator('#specimen-code');
      const note=listing.locator('.code-note');
      await note.locator('summary').focus();
      await page.keyboard.press('Enter');
      assert.equal(await note.evaluate(el=>el.open),true);
      assert.ok(await note.evaluate(el=>{
        const code=el.closest('.code-row').querySelector('pre').getBoundingClientRect();
        return Math.abs(el.querySelector('summary').getBoundingClientRect().bottom-code.bottom)<1;
      }),'Note must stay on its source line');
      if(javaScriptEnabled) {
        const original=await listing.locator('.code-source').textContent();
        await listing.locator('.code-wrap').click();
        assert.equal(await listing.locator('.code-wrap').getAttribute('aria-pressed'),'true');
        assert.ok(await listing.locator('.code-lines').evaluate(el=>el.scrollWidth<=el.clientWidth+1));
        assert.equal(await listing.locator('.code-source').textContent(),original);
      }
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
      await screenshot('code-note');
      const traversals=page.locator('.notebook-traversal');
      assert.equal(await traversals.count(),2);
      const traversal=traversals.first();
      assert.equal(await traversal.locator(':scope > .traversal-steps > li').count(),3);
      assert.equal(await traversals.last().locator(':scope > .traversal-steps > li').count(),2);
      for(const figure of await traversals.all()) {
        assert.ok(await figure.isVisible());
        assert.equal(await figure.locator('.traversal-steps').getAttribute('role'),'list');
        assert.match(await figure.locator('.traversal-steps').evaluate(el=>getComputedStyle(el).counterReset),/traversal/);
      }
      assert.equal(await traversal.locator('.traversal-branches li').first().evaluate(el=>getComputedStyle(el,'::before').content),'none');
      const step=traversal.locator('.traversal-steps > li').first();
      assert.equal(await step.evaluate(el=>getComputedStyle(el).paddingLeft),width<=600?'0px':'54px');
      await traversal.evaluate(el=>scrollTo(0,el.getBoundingClientRect().top+scrollY-32));
      await screenshot('traversal');
      const reference=traversal.getByRole('link',{name:'See the declaration'});
      await reference.focus();
      assert.ok(await reference.evaluate(el=>parseFloat(getComputedStyle(el).outlineWidth)>=2));
      await page.keyboard.press('Enter');
      await page.waitForURL(/#code-snippet--traversal-example-L2$/);
      assert.ok(await page.locator('#code-snippet--traversal-example-L2').evaluate(el=>{
        const line=el.closest('.code-row') || el;
        const bounds=line.getBoundingClientRect();
        return bounds.height>0 && bounds.top>=0 && bounds.top<innerHeight;
      }));
      if(size==='desktop' && javaScriptEnabled) {
        await page.emulateMedia({media:'print'});
        assert.equal(await traversal.evaluate(el=>getComputedStyle(el).backgroundColor),'rgb(255, 255, 255)');
        assert.equal(await traversal.locator('p').first().evaluate(el=>getComputedStyle(el).color),'rgb(0, 0, 0)');
        await traversal.evaluate(el=>scrollTo(0,el.getBoundingClientRect().top+scrollY-32));
        await screenshot('traversal-print');
        await page.emulateMedia({media:'screen'});
      }
      await inspectResults(page,screenshot,size==='desktop' && javaScriptEnabled);
      await inspectComparison(page,screenshot,width,size==='desktop' && javaScriptEnabled);
      await inspectDiagram(page,screenshot,size==='desktop' && javaScriptEnabled,enlarged);
      await inspectObservations(page,screenshot,width,size==='desktop' && javaScriptEnabled);
      await visit('feed.html');
      assert.equal(await page.locator('.journal-bookmark:visible').count(),2);
      assert.equal(await page.locator('.reading-tools').isVisible(),javaScriptEnabled);
      if(javaScriptEnabled) {
        assert.equal(await page.locator('.reading-tag-options button').count(),20);
        await page.getByRole('button',{name:'design',exact:true}).click();
        await page.getByRole('button',{name:'systems',exact:true}).click();
        assert.equal(await page.locator('.journal-bookmark:visible').count(),1);
        await page.reload();
        if(enlarged) await page.addStyleTag({content:'html { font-size: 200% !important; }'});
        assert.equal(await page.getByRole('button',{pressed:true}).count(),2);
        await page.getByRole('searchbox').fill('not-present-anywhere');
        assert.equal(await page.locator('.journal-bookmark:visible').count(),0);
        assert.ok(await page.locator('.reading-no-results').isVisible());
        await page.getByRole('button',{name:'Clear filters'}).click();
        assert.equal(await page.locator('.journal-bookmark:visible').count(),2);
        assert.ok(await page.getByRole('searchbox').evaluate(el=>el===document.activeElement));
      }
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
      await screenshot('many-tags');
    }
    assert.deepEqual(errors,[]);
  } finally {await context.close();}
}

(async()=>{
  fs.mkdirSync(artifacts,{recursive:true});
  const browser=await chromium.launch({headless:true, ...(process.env.BERLIN_BROWSER_EXECUTABLE ? {executablePath:process.env.BERLIN_BROWSER_EXECUTABLE} : {})});
  try {
    for(const variant of ['stress','empty']) {
      const project=await previewSpecimen(variant);
      try {
        for(const [size,width,height] of [['desktop',1536,1024],['tablet',820,1180],['phone',390,844],['small-phone',320,844]]) {
          for(const enabled of [true,false]) await inspect(browser,project,size,width,height,enabled);
        }
        await inspect(browser,project,'enlarged-text',820,1180,true,true);
        console.log(`${variant}: desktop/tablet/phone, no-JS and 200% text checks passed.`);
      } finally {await project.dispose();}
    }
    console.log(`Screenshots: ${artifacts}`);
  } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
