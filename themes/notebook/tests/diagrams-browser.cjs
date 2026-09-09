const assert = require('node:assert/strict');
const {contrast} = require('./contrast.cjs');

async function inspectDiagram(page, screenshot, print, enlarged) {
  const plate=page.locator('.notebook-diagram');
  assert.equal(await plate.count(),1);
  const image=plate.locator('figure > img');
  await image.evaluate(el=>el.decode());
  assert.ok(await image.getAttribute('alt'));
  const geometry=await image.evaluate(el=>{
    const image=el.getBoundingClientRect();
    const plate=el.closest('.notebook-diagram').getBoundingClientRect();
    const caption=el.parentElement.querySelector('figcaption').getBoundingClientRect();
    return {image,plate,caption,naturalWidth:el.naturalWidth,naturalHeight:el.naturalHeight};
  });
  assert.ok(geometry.image.width>0 && geometry.image.height>0);
  assert.ok(geometry.image.left>=geometry.plate.left && geometry.image.right<=geometry.plate.right);
  assert.ok(geometry.image.bottom<=geometry.caption.top);
  assert.ok(Math.abs(geometry.image.width/geometry.image.height-geometry.naturalWidth/geometry.naturalHeight)<.01);
  const colors=await plate.evaluate(el=>[...el.querySelectorAll('figcaption,.diagram-legend li,.diagram-actions a')].map(node=>{
    let parent=node;
    while(parent && getComputedStyle(parent).backgroundColor==='rgba(0, 0, 0, 0)') parent=parent.parentElement;
    return {ink:getComputedStyle(node).color,surface:getComputedStyle(parent).backgroundColor};
  }));
  for(const {ink,surface} of colors) assert.ok(contrast(ink,surface)>=4.5);
  const link=plate.getByRole('link',{name:'Open the full-size plot'});
  const imageUrl=await image.evaluate(el=>el.src);
  assert.equal(await link.evaluate(el=>el.href),imageUrl);
  await link.focus();
  assert.ok(await link.evaluate(el=>el===document.activeElement && parseFloat(getComputedStyle(el).outlineWidth)>=2));
  const article=page.url();
  await page.keyboard.press('Enter');
  await page.waitForURL(imageUrl);
  assert.equal(await page.locator('svg').count(),1);
  await page.goto(article);
  if(enlarged) await page.addStyleTag({content:'html { font-size: 200% !important; }'});
  await page.evaluate(()=>document.fonts.ready);
  await plate.locator('img').evaluate(el=>el.decode());
  await plate.evaluate(el=>scrollTo(0,el.getBoundingClientRect().top+scrollY-32));
  await screenshot('diagram');
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  const comparison=page.locator('#plot-defaults');
  assert.equal(await comparison.evaluate(el=>el.open),false);
  await comparison.locator('summary').focus();
  await page.keyboard.press('Enter');
  assert.equal(await comparison.evaluate(el=>el.open),true);
  await comparison.locator('img').evaluate(el=>el.decode());
  await comparison.evaluate(el=>scrollTo(0,el.getBoundingClientRect().top+scrollY-32));
  await screenshot('plot-defaults');
  await comparison.locator('summary').focus();
  await page.keyboard.press('Enter');
  assert.equal(await comparison.evaluate(el=>el.open),false);
  if(print) {
    await page.emulateMedia({media:'print'});
    assert.equal(await plate.locator('.diagram-actions').isVisible(),false);
    assert.equal(await plate.locator('figcaption').evaluate(el=>getComputedStyle(el).color),'rgb(0, 0, 0)');
    await plate.evaluate(el=>scrollTo(0,el.getBoundingClientRect().top+scrollY-32));
    await screenshot('diagram-print');
    await page.emulateMedia({media:'screen'});
  }
}

module.exports={inspectDiagram};
