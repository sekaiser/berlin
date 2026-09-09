const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {buildSpecimen} = require('./specimen.cjs');

test('code and worked-example presentation stays in component files', () => {
  for (const name of ['study', 'environment']) {
    const css = fs.readFileSync(path.join(__dirname, `../shared/styles/layouts/${name}.css`), 'utf8')
      // A layout may space ordinary figures while excluding code figures.
      .replaceAll('figure:not(.code-listing)', 'figure');
    assert.doesNotMatch(css, /\.(?:code-|json-explorer|medal-flow|flow-|notebook-traversal|traversal-|notebook-result|result-|notebook-comparison|comparison-|notebook-diagram|diagram-|notebook-observation|observation-|study-observation)/);
  }
});

test('entry presentation belongs to its component, not the overview layout', () => {
  const css = fs.readFileSync(path.join(__dirname, '../shared/styles/entries/overview.css'), 'utf8');
  assert.match(css, /@import "\.\.\/components\/entry\.css"/);
  assert.doesNotMatch(css, /\.(?:journal-entry|journal-excerpt|journal-date)/);
});

test('stress specimens publish long and untagged entries through the real pipeline', () => {
  const project = buildSpecimen('stress');
  try {
    const read = name => fs.readFileSync(path.join(project.output, name), 'utf8');
    assert.equal(JSON.parse(read('search/index.json')).documents.length, 5);
    const article = read('notes/long-title.html');
    assert.ok(article.includes('reader enlarges the text'));
    assert.equal([...article.matchAll(/class="paper-tag"/g)].length, 20);
    assert.match(article, /<table>/);
    const untagged = read('notes/untagged.html');
    assert.doesNotMatch(untagged, /class="study-topics"/);
    const code = read('notes/component-specimens.html');
    const contents = code.match(/<div class="toc">([\s\S]*?)<\/ul>/)[1];
    const destinations = [...contents.matchAll(/href="#([^"]+)"/g)].map(match => match[1]);
    const sections = [...code.matchAll(/<h2><a\b[^>]*\bid="([^"]+)"[^>]*>[\s\S]*?<\/h2>/g)].map(match => match[1]);
    assert.equal(sections.length, 9);
    assert.deepEqual(destinations, sections, 'The contents list exposes every specimen section');
    const authoring = path.join(__dirname, '../example/authoring');
    for (const name of fs.readdirSync(authoring).filter(name => name.endsWith('.org'))) {
      assert.ok(code.includes(`href="../examples/org/${name}" download`));
      assert.deepEqual(fs.readFileSync(path.join(project.output, 'examples/org', name)), fs.readFileSync(path.join(authoring, name)), 'Downloads contain the actual authoring source');
    }
    assert.match(code, /id="specimen-line-2"/);
    assert.match(code, /class="code-note"/);
    assert.match(code, /class="code-caption"/);
    assert.equal([...code.matchAll(/class="notebook-observation"/g)].length, 2);
    assert.match(code, /role="note" aria-label="Zero is still a value"/);
    assert.match(code, /id="code-snippet--observation-example-L2"/);
    assert.match(code, /href="#code-snippet--observation-example-L2"/);
    const source = fs.readFileSync(path.join(__dirname, '../example/authoring/traversal.org'), 'utf8');
    const figure = source.match(/<figure[\s\S]*?<\/figure>/)[0];
    assert.ok(code.includes(figure), 'The Org authoring example and published specimen must agree');
    assert.equal([...code.matchAll(/class="notebook-traversal"/g)].length, 2);
    for (const line of [2, 3]) assert.ok(code.includes(`id="code-snippet--traversal-example-L${line}"`));
    assert.doesNotMatch(code, /rust-exhibits/);
    const results = fs.readFileSync(path.join(__dirname, '../example/authoring/results.org'), 'utf8');
    for (const [figure] of results.matchAll(/<figure[\s\S]*?<\/figure>/g)) {
      assert.ok(code.includes(figure), 'Authored result figures survive the real website build');
    }
    const comparison = fs.readFileSync(path.join(__dirname, '../example/authoring/comparison.org'), 'utf8');
    const pair = comparison.match(/<figure[\s\S]*?<\/figure>/)[0];
    assert.ok(code.includes(pair), 'The Org comparison and published specimen must agree');
    const values = [...pair.matchAll(/<pre[^>]*><code>([\s\S]*?)<\/code><\/pre>/g)].map(match=>JSON.parse(match[1]));
    assert.equal(values.length,2);
    assert.deepEqual(values[0],values[1], 'The whitespace comparison must not change the data');
    assert.match(code,/class="notebook-diagram"/);
    assert.match(code,/alt="Line plot connecting the synthetic points/);
    for (const name of ['notebook-plot.svg', 'sample-plot.svg']) {
      assert.deepEqual(fs.readFileSync(path.join(project.output,'static/diagrams',name)),
        fs.readFileSync(path.join(__dirname,'../example/static/diagrams',name)));
    }
    assert.equal([...read('feed.html').matchAll(/class="journal-bookmark"/g)].length, 2);
    const home = read('index.html');
    const entries = html => [...html.matchAll(/<article class="journal-entry(?: journal-entry--featured)?">([\s\S]*?)<\/article>/g)].map(match => match[1]);
    const details = entry => ({
      title: entry.match(/<h[23]>([\s\S]*?)<\/h[23]>/)[1],
      summary: entry.match(/<div class="journal-excerpt">([\s\S]*?)<\/div>/)[1],
      date: entry.match(/<time datetime="([^"]+)">([^<]+)<\/time>/).slice(1),
    });
    assert.equal([...home.matchAll(/class="journal-entry journal-entry--featured"/g)].length, 2);
    assert.equal(entries(home).length, 5);
    assert.deepEqual(entries(home).map(details), entries(read('notes.html')).map(details));
    for (const page of ['notes.html', 'tags/design.html', 'notes/component-specimens.html']) {
      assert.doesNotMatch(read(page), /journal-entry--featured/);
    }
  } finally {project.dispose();}
});

test('empty specimens publish useful collection pages without invented entries', () => {
  const project = buildSpecimen('empty');
  try {
    const read = name => fs.readFileSync(path.join(project.output, name), 'utf8');
    assert.equal(JSON.parse(read('search/index.json')).documents.length, 0);
    assert.match(read('feed.html'), /No saved references yet/);
    assert.doesNotMatch(read('feed.html'), /class="journal-bookmark"/);
    assert.doesNotMatch(read('notes.html'), /class="journal-entry"/);
    assert.ok(fs.existsSync(path.join(project.output, 'index.html')));
  } finally {project.dispose();}
});
