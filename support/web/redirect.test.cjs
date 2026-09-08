const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const template = fs.readFileSync(path.join(__dirname, '../../cli/tasks/website/redirect.tera'), 'utf8');
const script = template.match(/<script>([\s\S]*?)<\/script>/)[1];

test('redirect preserves query and fragment without accepting an input destination', () => {
  for (const [search, hash] of [['', ''], ['?from=old', '#details'], ['?next=https://other.test', '#a%20b']]) {
    let replaced;
    const destination = 'https://example.com/notebook/notes/current.html';
    vm.runInNewContext(script, {
      URL,
      document: {querySelector: () => ({href: destination})},
      location: {search, hash, replace: value => { replaced = value; }},
    });
    assert.equal(replaced, destination + search + hash);
  }
});

test('redirect retains canonical, no-index, no-script refresh and ordinary link', () => {
  assert.ok(template.includes('<link rel="canonical" href="{{destination}}">'));
  assert.ok(template.includes('content="noindex, follow"'));
  assert.ok(template.includes('http-equiv="refresh" content="0;url={{destination}}"'));
  assert.ok(template.includes('<a href="{{destination}}">'));
});
