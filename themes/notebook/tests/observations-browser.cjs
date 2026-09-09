const assert = require('node:assert/strict');
const {contrast} = require('./contrast.cjs');

async function inspectObservations(page, screenshot, width, print) {
  const notes = page.locator('.notebook-observation');
  assert.equal(await notes.count(), 2);
  for (const note of await notes.all()) {
    assert.ok(await note.isVisible());
    assert.equal(await note.getAttribute('role'), 'note');
    assert.equal(await note.getAttribute('aria-label'), await note.locator('.observation-title').innerText());
    const geometry = await note.evaluate(el => {
      const title = el.querySelector('.observation-title').getBoundingClientRect();
      const body = el.querySelector('.observation-body').getBoundingClientRect();
      return {title, body, position: getComputedStyle(el).position};
    });
    assert.equal(geometry.position, 'static', 'Authored observations stay in reading order');
    if (width <= 700) assert.ok(geometry.title.bottom <= geometry.body.top);
    else assert.ok(geometry.title.right < geometry.body.left);
    const colors = await note.evaluate(el => [...el.querySelectorAll('p,code,a')].map(node => {
      let surface = node;
      while (surface && getComputedStyle(surface).backgroundColor === 'rgba(0, 0, 0, 0)') surface = surface.parentElement;
      return {ink: getComputedStyle(node).color, surface: getComputedStyle(surface).backgroundColor};
    }));
    for (const color of colors) assert.ok(contrast(color.ink, color.surface) >= 4.5);
    assert.ok(await note.evaluate(el => el.scrollWidth <= el.clientWidth + 1));
  }
  const note = notes.first();
  assert.equal(await notes.last().locator('.observation-actions').count(), 0);
  await note.evaluate(el => scrollTo(0, el.getBoundingClientRect().top + scrollY - 32));
  await screenshot('observation');
  const reference = note.getByRole('link', {name: 'See the recorded value'});
  await reference.focus();
  assert.ok(await reference.evaluate(el => parseFloat(getComputedStyle(el).outlineWidth) >= 2));
  await page.keyboard.press('Enter');
  await page.waitForURL(/#code-snippet--observation-example-L2$/);
  assert.ok(await page.locator('#code-snippet--observation-example-L2').evaluate(el => {
    const row = (el.closest('.code-row') || el).getBoundingClientRect();
    return row.top >= 0 && row.top < innerHeight;
  }));
  if (print) {
    await page.emulateMedia({media: 'print'});
    assert.ok(await note.locator('.observation-body').isVisible());
    assert.equal(await reference.isVisible(), false);
    assert.equal(await note.locator('.observation-title').evaluate(el => getComputedStyle(el).color), 'rgb(0, 0, 0)');
    await note.evaluate(el => scrollTo(0, el.getBoundingClientRect().top + scrollY - 32));
    await screenshot('observation-print');
    await page.emulateMedia({media: 'screen'});
  }
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
}

module.exports = {inspectObservations};
