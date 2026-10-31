const assert = require('node:assert/strict');
const {contrast} = require('./contrast.cjs');

async function inspectComparison(page, screenshot, width, print) {
  const figure=page.locator('.notebook-comparison');
  assert.equal(await figure.count(),1);
  assert.ok(await figure.isVisible());
  const colors=await figure.evaluate(el=>[...el.querySelectorAll('figcaption span, .comparison-label, p, pre code')].map(node=>{
    let parent=node;
    while(parent && getComputedStyle(parent).backgroundColor==='rgba(0, 0, 0, 0)') parent=parent.parentElement;
    return {ink:getComputedStyle(node).color,surface:getComputedStyle(parent).backgroundColor};
  }));
  for(const {ink,surface} of colors) assert.ok(contrast(ink,surface)>=4.5,`Low comparison contrast: ${ink} on ${surface}`);
  const panels=figure.locator('.comparison-panel');
  assert.equal(await panels.count(),2);
  assert.deepEqual(await panels.locator('.comparison-label').allTextContents(),['Indented','Compact']);
  const sources=await panels.locator('pre code').allTextContents();
  assert.deepEqual(JSON.parse(sources[0]),JSON.parse(sources[1]));
  const bounds=await panels.evaluateAll(elements=>elements.map(el=>{
    const {top,bottom,left,right}=el.getBoundingClientRect();
    return {top,bottom,left,right};
  }));
  if(width<=700) {
    assert.ok(bounds[0].bottom<=bounds[1].top);
    assert.equal(bounds[0].left,bounds[1].left);
  } else {
    assert.equal(bounds[0].top,bounds[1].top);
    assert.ok(bounds[0].right<bounds[1].left);
  }
  // The code variant must stack on a tablet even though the ordinary compact
  // data specimens above still fit side by side at that viewport size.
  await figure.evaluate(el=>el.classList.add('notebook-comparison--code'));
  const codeBounds=await panels.evaluateAll(elements=>elements.map(el=>{
    const {top,bottom,left,right}=el.getBoundingClientRect();
    return {top,bottom,left,right};
  }));
  if(width<=820) {
    assert.ok(codeBounds[0].bottom<=codeBounds[1].top,'Narrow code comparisons stack');
    assert.equal(codeBounds[0].left,codeBounds[1].left);
  } else {
    assert.equal(codeBounds[0].top,codeBounds[1].top);
    assert.ok(codeBounds[0].right<codeBounds[1].left);
  }
  await figure.evaluate(el=>el.classList.remove('notebook-comparison--code'));
  const compact=panels.last().locator('pre');
  assert.ok(await compact.evaluate(el=>el.scrollWidth>el.clientWidth));
  await compact.scrollIntoViewIfNeeded();
  await compact.focus();
  assert.ok(await compact.evaluate(el=>el===document.activeElement && parseFloat(getComputedStyle(el).outlineWidth)>=2));
  await page.keyboard.press('ArrowRight');
  await page.waitForTimeout(250);
  assert.ok(await compact.evaluate(el=>el.scrollLeft>0));
  assert.deepEqual(await panels.locator('pre code').allTextContents(),sources);
  for(const pre of await panels.locator('pre').all()) {
    assert.ok(await pre.evaluate(el=>parseFloat(getComputedStyle(el).fontSize)>=14));
  }
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  await compact.evaluate(el=>{el.scrollLeft=0;el.blur();});
  await figure.evaluate(el=>scrollTo(0,el.getBoundingClientRect().top+scrollY-32));
  await screenshot('comparison');
  if(print) {
    await page.emulateMedia({media:'print'});
    assert.equal(await compact.evaluate(el=>getComputedStyle(el).whiteSpace),'pre-wrap');
    assert.equal(await compact.evaluate(el=>getComputedStyle(el).color),'rgb(0, 0, 0)');
    await figure.evaluate(el=>scrollTo(0,el.getBoundingClientRect().top+scrollY-32));
    await screenshot('comparison-print');
    await page.emulateMedia({media:'screen'});
  }
}

module.exports={inspectComparison};
