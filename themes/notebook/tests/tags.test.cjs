const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {contrast} = require('./contrast.cjs');
const {index} = require('../shared/static/js/tag-colors.js');

test('64 distinct tag surfaces have readable ink and clear edges', () => {
  const css = fs.readFileSync(path.join(__dirname, '../shared/styles/components/tag-palette.css'), 'utf8');
  const colors = [...css.matchAll(/--tag-(\d+)-paper: (#[a-f0-9]{6}); --tag-\1-ink: (#[a-f0-9]{6}); --tag-\1-edge: (#[a-f0-9]{6});/g)];
  assert.equal(colors.length, 64);
  assert.equal(new Set(colors.map(row => row[2])).size, 64);
  const rgb = hex => `rgb(${hex.slice(1).match(/../g).map(value => parseInt(value, 16)).join(',')})`;
  for (const [slot, row] of colors.entries()) {
    const [,id,paper,ink,edge] = row;
    assert.equal(Number(id), slot);
    assert.ok(contrast(rgb(ink), rgb(paper)) >= 4.5, `Slot ${id}: text contrast`);
    assert.ok(contrast(rgb(edge), rgb(paper)) >= 3, `Slot ${id}: edge contrast`);
    assert.ok(css.includes(`.paper-tag[data-tag-color="${id}"]`));
  }
});

test('palette includes 32 named bases with two tones each', () => {
  const css = fs.readFileSync(path.join(__dirname, '../shared/styles/components/tag-palette.css'), 'utf8');
  const bases = new Map();
  for (const [,name,tone] of css.matchAll(/\/\* ([a-z]+) ([12]) \*\//g)) {
    const tones = bases.get(name) || [];
    tones.push(Number(tone));
    bases.set(name, tones);
  }
  assert.equal(bases.size, 32);
  for (const tones of bases.values()) assert.deepEqual(tones, [1, 2]);
});

test('client slots match the fixed server hash vectors and ignore label case', () => {
  for (const [label,slot] of [['',5],['rust',39],['RUST',39],['café',9],['東京',47],['🦀',38],['system-design',17],['org-roam',21]]) {
    assert.equal(index(label), slot, label);
  }
  const assigned = Array.from({length:10000}, (_,i) => index(`tag-${i}`));
  assert.equal(new Set(assigned).size,64);
  assert.ok(assigned.every(slot => Number.isInteger(slot) && slot >= 0 && slot < 64));
});

test('unselected Reading tags retain their neutral override', () => {
  const css = fs.readFileSync(path.join(__dirname, '../shared/styles/entries/reading.css'), 'utf8');
  assert.match(css, /\.paper-tag\[aria-pressed="false"\]\s*\{\s*--tag-paper: transparent;/);
  const reading = fs.readFileSync(path.join(__dirname, '../shared/static/js/reading.js'), 'utf8');
  assert.match(reading, /button.dataset.tagColor = tagColors.get\(tag\)/);
});

test('tag palette families provide readable ink and reusable surface/edge tokens', () => {
  const css = fs.readFileSync(path.join(__dirname, '../shared/styles/tokens.css'), 'utf8');
  const rgb = hex => `rgb(${hex.match(/../g).map(value => parseInt(value, 16)).join(',')})`;
  const token = name => {
    const value = css.match(new RegExp(`--${name}: #([a-f0-9]{6});`));
    assert.ok(value, `Missing ${name}`);
    return value[1];
  };
  for (const family of ['moss', 'sky', 'apricot', 'lavender', 'ochre', 'rose']) {
    token(`${family}-edge`);
    assert.ok(contrast(rgb(token(`${family}-ink`)), rgb(token(family))) >= 4.5, `${family}: insufficient text contrast`);
  }
});
