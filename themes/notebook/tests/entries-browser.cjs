const assert = require('node:assert/strict');
const {contrast} = require('./contrast.cjs');

async function inspectEntries(page, screenshot, width, print, enlarged) {
  const selected = page.locator('.journal-entry--featured');
  assert.equal(await selected.count(), 2);
  assert.equal(await page.locator('.journal-entry:not(.journal-entry--featured) .journal-entry-label').count(), 0);
  for (const entry of await selected.all()) {
    assert.ok(await entry.isVisible());
    assert.equal(await entry.locator('.journal-entry-label').textContent(), 'Selected note');
    assert.equal(await entry.evaluate(el => getComputedStyle(el).backgroundColor), 'rgba(0, 0, 0, 0)');
    const heading = entry.locator('h3');
    assert.equal(await heading.evaluate(el => getComputedStyle(el).fontWeight), '400');
    const colors = await entry.evaluate(el => [...el.querySelectorAll('h3, .journal-entry-label, .journal-date, .journal-excerpt')].map(node => {
      let surface = node;
      while (surface && getComputedStyle(surface).backgroundColor === 'rgba(0, 0, 0, 0)') surface = surface.parentElement;
      return {ink: getComputedStyle(node).color, surface: getComputedStyle(surface).backgroundColor};
    }));
    for (const color of colors) assert.ok(contrast(color.ink, color.surface) >= 4.5);
    const image = entry.locator('img');
    const illustrated = await image.count() > 0;
    if (illustrated) {
      await image.evaluate(el => el.decode());
      assert.ok(await image.getAttribute('alt'));
      assert.equal(await entry.locator('.journal-entry-visual').getAttribute('href'), await heading.locator('a').getAttribute('href'));
      const geometry = await image.evaluate(el => ({
        image: el.getBoundingClientRect(), copy: el.closest('.journal-entry').querySelector('.journal-entry-copy').getBoundingClientRect(),
        ratio: el.naturalWidth / el.naturalHeight,
      }));
      assert.ok(Math.abs(geometry.image.width / geometry.image.height - geometry.ratio) < .01);
      if (width <= 600) assert.ok(geometry.image.bottom <= geometry.copy.top);
      else assert.ok(geometry.image.right < geometry.copy.left);
    }
    await entry.evaluate(el => scrollTo(0, el.getBoundingClientRect().top + scrollY - 32));
    await screenshot(illustrated ? 'featured-illustrated' : 'featured-text');
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  }
  const last = selected.last();
  const title = last.locator('h3 a');
  const target = await title.getAttribute('href');
  const home = page.url();
  await title.focus();
  assert.ok(await title.evaluate(el => parseFloat(getComputedStyle(el).outlineWidth) >= 2));
  await page.keyboard.press('Enter');
  await page.waitForURL(target);
  assert.equal(await page.locator('h1').textContent(), 'Reading a small plot');
  await page.goto(home);
  if (enlarged) await page.addStyleTag({content:'html { font-size: 200% !important; }'});
  await page.evaluate(() => document.fonts.ready);
  if (print) {
    await page.emulateMedia({media:'print'});
    assert.equal(await last.locator('.journal-entry-label').evaluate(el => getComputedStyle(el).color), 'rgb(0, 0, 0)');
    await last.evaluate(el => scrollTo(0, el.getBoundingClientRect().top + scrollY - 32));
    await screenshot('featured-print');
    await page.emulateMedia({media:'screen'});
  }
}

module.exports = {inspectEntries};
