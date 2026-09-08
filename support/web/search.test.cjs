const {test} = require('node:test');
const assert = require('node:assert/strict');
const {prepare, search, excerpt, resultUrl} = require('./search.js');
const data = prepare({schema_version: 1, documents: [
  {title: 'Processing JSON', kind: 'article', tags: ['Rust'], path: 'notes/processing-json.html', sections: [
    {heading: '', text: 'A learning exercise.'},
    {heading: 'Extracting medals', fragment: 'medals', text: 'Navigate countryObject using serde_json::from_str and collect results.'}
  ]},
  {title: 'Medals', kind: 'guide', tags: [], path: 'notes/medals.html', sections: [{heading: '', text: 'An introduction.'}]}
]});
test('matches body/code and tags with AND semantics, returning the relevant section', () => {
  const results = search(data, 'RUST countryObject');
  assert.equal(results.length, 1);
  assert.equal(results[0].section.fragment, 'medals');
  assert.equal(search(data, 'serde_json::from_str').length, 1);
  assert.equal(search(data, 'rust absent').length, 0);
  assert.equal(search(data, '   ').length, 0);
});
test('ranks title matches first and returns a document only once', () => {
  assert.equal(search(data, 'medals')[0].document.title, 'Medals');
  assert.equal(search(data, 'rust')[0].section.heading, '');
  assert.equal(search(data, 'rust').length, 1);
});
test('normalizes Unicode and treats punctuation as text, not regex', () => {
  const unicode = prepare({schema_version: 1, documents: [{title: 'Café', tags: [], path: 'notes/cafe.html', sections: [{heading: '', text: '<script> [.*]'}]}]});
  assert.equal(search(unicode, 'Cafe\u0301').length, 1);
  assert.equal(search(unicode, '[.*]').length, 1);
  assert.equal(search(unicode, '<script>')[0].section.text, '<script> [.*]');
});
test('supports Pages prefixes and rejects unsafe routes', () => {
  assert.equal(resultUrl('https://example.com/notebook/search/index.json', data[0], data[0].sections[1]), 'https://example.com/notebook/notes/processing-json.html#medals');
  assert.throws(() => resultUrl('https://example.com/search/index.json', {path: '//evil.test'}, {}));
  assert.equal(resultUrl('https://example.com/search/index.json', data[0], {fragment:'a%20b'}), 'https://example.com/notes/processing-json.html#a%20b');
});
test('excerpts show matching passages without interpreting markup', () => {
  const text = 'Earlier text. '.repeat(50) + 'countryObject <script>example</script> ' + 'After. '.repeat(50);
  const value = excerpt(text, 'countryObject');
  assert.ok(value.includes('countryObject <script>'));
  assert.ok(value.startsWith('…'));
  assert.ok(value.endsWith('…'));
  assert.ok(value.length <= 242);
});

test('excerpts end at word boundaries and keep the matching word in view', () => {
  assert.equal(excerpt('One two enormousword follows here.', '', 12), 'One two…');
  assert.equal(excerpt('One two', '', 7), 'One two');
  assert.equal(excerpt('   ', 'word'), '');
  const text = 'unusuallylongcontextword '.repeat(20) + 'needle useful explanation follows';
  assert.ok(excerpt(text, 'needle', 30).includes('needle'));
  assert.equal(excerpt('extraordinarilylongidentifier more', 'identifier', 8), 'extraordinarilylongidentifier…');
  assert.equal(excerpt('Café résumé next', '', 11), 'Café résumé…');
});

// A deliberately small DOM double exercises the client contract without claiming
// browser layout coverage. Disallow innerHTML so unsafe rendering fails the test.
async function client({query = '', fail = false, tagPaths = {rust:'tags/rust.html'}, note = {title:'<script>Example</script>', kind:'article', tags:['rust'], path:'notes/example.html', sections:[{heading:'Details', fragment:'details', text:'countryObject <img onerror=alert(1)>'}]} } = {}) {
  const vm = require('node:vm');
  const fs = require('node:fs');
  function element(tagName) {
    let text = '';
    return {tagName, dataset:{}, children: [], listeners: {}, hidden: true,
      get textContent() { return text + this.children.map(child => child.textContent).join(''); },
      set textContent(value) { text = value; this.children = []; },
      append(...items) { this.children.push(...items); },
      replaceChildren(...items) { this.children = items; },
      addEventListener(name, fn) { this.listeners[name] = fn; },
      set innerHTML(_) { throw Error('Do not interpret indexed text as HTML'); }
    };
  }
  const form = element(), input = {...element(), value:'', maxLength:200}, status = element(), results = element();
  const selectors = {'form':form, 'input[type="search"]':input, '[data-search-status]':status, '[data-search-results]':results};
  const root = {dataset:{searchIndex:'https://example.com/notebook/search/index.json?v=hash'}, querySelector: key => selectors[key]};
  const document = {querySelectorAll: () => [root], createElement: element, createTextNode: text => ({textContent:text})};
  const location = {href: 'https://example.com/notebook/search.html?q=' + encodeURIComponent(query)};
  const window = {document, addEventListener() {}};
  vm.runInNewContext(fs.readFileSync(require.resolve('./search.js'), 'utf8'), {
    document, window, location, URL,
    history:{replaceState(_a, _b, url) { location.href = url.href; }},
    fetch: async () => ({ok:!fail, json: async () => ({schema_version:1, tag_paths:tagPaths, documents:[note]})})
  });
  await new Promise(resolve => setImmediate(resolve));
  return {form, input, status, results, location};
}
test('client restores URL queries, renders safe section links, and supports clearing', async () => {
  const c = await client({query:'countryObject'});
  assert.equal(c.form.hidden, false);
  assert.equal(c.results.children.length, 1);
  const heading = c.results.children[0].children[1];
  assert.equal(heading.children[0].textContent, '<script>Example</script>');
  assert.equal(heading.children[0].href, 'https://example.com/notebook/notes/example.html#details');
  c.input.value = 'not present'; c.input.listeners.input();
  assert.equal(c.results.children.length, 0);
  assert.match(c.status.textContent, /No matching notes/);
  assert.match(c.location.href, /q=not\+present/);
  c.input.value = ''; c.form.listeners.submit({preventDefault(){}});
  assert.equal(new URL(c.location.href).searchParams.has('q'), false);
  assert.match(c.status.textContent, /All words must match/);
});
test('client explains a failed fetch without offering an inert form', async () => {
  const c = await client({fail:true});
  assert.equal(c.form.hidden, true);
  assert.match(c.status.textContent, /Search could not load/);
});

test('results use safe highlighted text and the existing linked paper-tag markup', async () => {
  const c = await client({query:'countryObject'});
  const result = c.results.children[0];
  const passage = result.children.find(child => child.className === 'search-result-excerpt');
  assert.equal(passage.children[0].tagName, 'mark');
  assert.equal(passage.children[0].textContent, 'countryObject');
  assert.equal(passage.textContent, 'countryObject <img onerror=alert(1)>');
  assert.ok(passage.children.every(child => !child.tagName || child.tagName === 'mark'));
  const tags = result.children.find(child => child.className === 'search-result-tags');
  assert.equal(tags.children[0].className, 'paper-tag');
  assert.equal(tags.children[0].dataset.tag, 'rust');
  assert.equal(tags.children[0].href, 'https://example.com/notebook/tags/rust.html');
  assert.equal(tags.children[0].children[0].tagName, 'span');
  assert.equal(tags.children[0].textContent, 'rust');
});

test('highlighting preserves original Unicode spelling and handles literal punctuation', async () => {
  const c = await client({query:'café [.*]', note:{title:'Cafe\u0301 [.*]', kind:'article', tags:[], path:'notes/cafe.html', sections:[{heading:'', text:'A Cafe\u0301 [.*] example.'}]}});
  const link = c.results.children[0].children[1].children[0];
  assert.equal(link.textContent, 'Cafe\u0301 [.*]');
  assert.deepEqual(link.children.filter(child => child.tagName === 'mark').map(child => child.textContent), ['Cafe\u0301', '[.*]']);
});

test('missing or unsafe tag paths remain noninteractive labels', async () => {
  for (const tagPaths of [{}, {rust:'https://evil.test/'}, {rust:'tags/../../private'}]) {
    const c = await client({query:'rust', tagPaths});
    const tag = c.results.children[0].children.at(-1).children[0];
    assert.equal(tag.tagName, 'span');
    assert.equal(tag.href, undefined);
  }
});
