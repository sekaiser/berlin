const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

async function downloadBytes(download) {
  let timeout;
  try {
    const file = await Promise.race([
      download.path(),
      new Promise((_, reject) => {timeout = setTimeout(() => reject(new Error(`Download did not finish: ${download.suggestedFilename()}`)), 15000);}),
    ]);
    return fs.readFileSync(file);
  } finally {clearTimeout(timeout);}
}

async function inspectReference(page, screenshot, width, javaScriptEnabled) {
  const contents = page.locator(javaScriptEnabled ? '.study-contents' : '.frontmatter .toc');
  if (javaScriptEnabled && !await contents.evaluate(el => el.open)) {
    await contents.locator('summary').focus();
    await page.keyboard.press('Enter');
  }
  const headings = await page.locator('.study-prose > h2 > a[id]').evaluateAll(nodes => nodes.map(el => `#${el.id}`));
  assert.equal(headings.length, 9);
  assert.deepEqual(await contents.locator('a').evaluateAll(nodes => nodes.map(el => el.getAttribute('href'))), headings);
  await page.locator('#choose').evaluate(el => scrollTo(0, el.getBoundingClientRect().top + scrollY - 32));
  await screenshot('component-reference');
  const link = contents.getByRole('link', {name:'Authored observations', exact:true});
  await link.focus();
  await page.keyboard.press('Enter');
  await page.waitForURL(/#authored-observations$/);
  assert.ok(await page.locator('#authored-observations').evaluate(el => {
    const top = el.getBoundingClientRect().top;
    return top >= 0 && top < innerHeight;
  }));
  if (javaScriptEnabled && width < 1200) {
    // Restore the normal collapsed mobile margin after checking its navigation.
    await contents.evaluate(el => {el.open = false;});
  }
  for (const name of ['observations.org', 'traversal.org', 'diagrams.org', 'comparison.org', 'results.org']) {
    const downloadLink = page.getByRole('link', {name:`Download ${name}`, exact:true});
    assert.ok(await downloadLink.isVisible());
    await downloadLink.focus();
    assert.ok(await downloadLink.evaluate(el => parseFloat(getComputedStyle(el).outlineWidth) >= 2));
    // File bytes do not depend on viewport size. Exercise full native downloads
    // on desktop with JS and on a phone without JS; check every link at all sizes.
    if (!((width === 1536 && javaScriptEnabled) || (width === 390 && !javaScriptEnabled))) continue;
    const received = page.waitForEvent('download');
    await page.keyboard.press('Enter');
    const download = await received;
    assert.equal(download.suggestedFilename(), name);
    assert.deepEqual(await downloadBytes(download), fs.readFileSync(path.join(__dirname, '../example/authoring', name)));
    // The browser context owns and removes its downloaded files when closed.
  }
  const figures = page.locator('.notebook-traversal, .notebook-result, .notebook-comparison, .notebook-diagram');
  const gap = width <= 600 ? '30px' : '42px';
  const margins = () => figures.evaluateAll(nodes => nodes.map(el => {
    const style = getComputedStyle(el);
    return [style.marginTop, style.marginBottom];
  }));
  assert.equal(await figures.count(), 6);
  for (const values of await margins()) assert.deepEqual(values, [gap, gap]);
  // addStyleTag waits for page-world events, which cannot run with JS disabled.
  const previous = await page.evaluate(() => {
    const style = document.body.style;
    const previous = [style.getPropertyValue('--figure-gap'), style.getPropertyPriority('--figure-gap')];
    style.setProperty('--figure-gap', '56px');
    return previous;
  });
  try {
    for (const values of await margins()) assert.deepEqual(values, ['56px', '56px']);
  } finally {
    await page.evaluate(([value, priority]) => {
      if (value) document.body.style.setProperty('--figure-gap', value, priority);
      else document.body.style.removeProperty('--figure-gap');
    }, previous);
  }
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  console.log(`Reference checked (${width}px, JS ${javaScriptEnabled})`);
}

module.exports = {inspectReference};
