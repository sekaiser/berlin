const assert = require('node:assert/strict');
const {contrast} = require('./contrast.cjs');

async function inspectTagPalette(page, screenshot) {
  const palette = page.locator('[aria-label="64 tag color specimens"]');
  const colors = await palette.locator('.paper-tag').evaluateAll(tags => tags.map(tag => ({
    slot: tag.dataset.tagColor,
    paper: getComputedStyle(tag, '::before').backgroundColor,
    ink: getComputedStyle(tag).color,
    edge: getComputedStyle(tag, '::before').borderTopColor,
  })));
  assert.equal(colors.length, 64);
  assert.equal(new Set(colors.map(tag => tag.paper)).size, 64);
  for (const tag of colors) {
    assert.ok(contrast(tag.ink, tag.paper) >= 4.5, `Slot ${tag.slot}: ink`);
    assert.ok(contrast(tag.edge, tag.paper) >= 3, `Slot ${tag.slot}: edge`);
  }
  await palette.evaluate(element => scrollTo(0, element.getBoundingClientRect().top + scrollY - 24));
  await screenshot('tag-palette');
}

async function inspectTagFilter(page, name) {
  const link = page.locator(`[data-reading-list] .publication-resource-items .paper-tag[data-tag="${name}"]`).first();
  const button = page.getByRole('button', {name, exact:true});
  assert.equal(await button.getAttribute('data-tag-color'), await link.getAttribute('data-tag-color'));
  const paper = element => getComputedStyle(element, '::before').backgroundColor;
  assert.equal(await button.evaluate(paper), 'rgba(0, 0, 0, 0)');
  await button.click();
  assert.equal(await button.evaluate(paper), await link.evaluate(paper));
}

module.exports = {inspectTagPalette, inspectTagFilter};
