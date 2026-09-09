// Shared result contracts, exercised against real output with and without JavaScript.
const assert = require('node:assert/strict');

const {contrast} = require('./contrast.cjs');

async function inspectResults(page, screenshot, print) {
  const figures=page.locator('.notebook-result');
  assert.equal(await figures.count(),2);
  for(const figure of await figures.all()) {
    assert.ok(await figure.isVisible());
    assert.equal(await figure.getByRole('table').count(),1);
    assert.ok(await figure.locator('table caption').textContent());
    const samples=await figure.evaluate(el=>[...el.querySelectorAll('figcaption span, th, td, summary')].map(node=>{
      let parent=node;
      while(parent && getComputedStyle(parent).backgroundColor==='rgba(0, 0, 0, 0)') parent=parent.parentElement;
      return {color:getComputedStyle(node).color,background:getComputedStyle(parent).backgroundColor};
    }));
    for(const {color,background} of samples) assert.ok(contrast(color,background)>=4.5,`${color} on ${background}`);
    const viewport=figure.locator('.result-table');
    await viewport.focus();
    assert.ok(await viewport.evaluate(el=>el===document.activeElement && parseFloat(getComputedStyle(el).outlineWidth)>=2));
    // Exercise an optional cell link in the test DOM, then restore the authored table.
    const cell=figure.locator('tbody th').first();
    await cell.evaluate(el=>{
      const link=document.createElement('a');
      link.href=`#${el.closest('figure').querySelector('figcaption').id}`;
      link.append(...Array.from(el.childNodes));
      el.append(link);
    });
    const link=cell.locator('a');
    await link.focus();
    const ring=await link.evaluate(el=>({
      width:parseFloat(getComputedStyle(el).outlineWidth),
      color:getComputedStyle(el).outlineColor,
      background:getComputedStyle(el.closest('.notebook-result')).backgroundColor,
    }));
    assert.ok(ring.width>=2 && contrast(ring.color,ring.background)>=3, 'Cell links need a visible focus ring on both surfaces');
    await link.evaluate(el=>el.replaceWith(...Array.from(el.childNodes)));
  }
  const forest=figures.first();
  assert.deepEqual(await forest.locator('tbody td').allTextContents(),['12','3','0']);
  const source=forest.locator('.result-source');
  assert.equal(await source.evaluate(el=>el.open),false);
  await source.locator('summary').focus();
  await page.keyboard.press('Enter');
  assert.equal(await source.evaluate(el=>el.open),true);
  assert.equal(await source.locator('code').textContent(),'accepted=12\ndeferred=3\nrejected=0');
  await page.keyboard.press('Space');
  assert.equal(await source.evaluate(el=>el.open),false);
  assert.equal(await figures.last().locator('.result-source').count(),0);
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  for(const [name,figure] of [['forest',forest],['light',figures.last()]]) {
    await figure.evaluate(el=>scrollTo(0,el.getBoundingClientRect().top+scrollY-32));
    await screenshot(`result-${name}`);
  }
  if(print) {
    await page.emulateMedia({media:'print'});
    assert.equal(await forest.evaluate(el=>getComputedStyle(el).backgroundColor),'rgb(255, 255, 255)');
    assert.ok(await source.locator('pre').isVisible());
    await forest.evaluate(el=>scrollTo(0,el.getBoundingClientRect().top+scrollY-32));
    await screenshot('result-print');
    await page.emulateMedia({media:'screen'});
  }
}

async function inspectWideResult(page, screenshot) {
  const viewport=page.locator('.notebook-result > .result-table');
  assert.ok(await viewport.evaluate(el=>el.scrollWidth>el.clientWidth));
  await viewport.scrollIntoViewIfNeeded();
  await viewport.focus();
  assert.ok(await viewport.evaluate(el=>el===document.activeElement));
  await page.keyboard.press('ArrowRight');
  // Native keyboard scrolling may animate, including with page scripts disabled.
  await page.waitForTimeout(250);
  assert.ok(await viewport.evaluate(el=>el.scrollLeft>0),'The focused result table must scroll with ArrowRight');
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  await page.locator('.notebook-result').evaluate(el=>scrollTo(0,el.getBoundingClientRect().top+scrollY-32));
  await screenshot('result-wide');
}

module.exports={inspectResults,inspectWideResult};
