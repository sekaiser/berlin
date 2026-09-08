const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const script = fs.readFileSync(path.join(__dirname, 'live.js'), 'utf8');

test('preview polls with GET, reloads after a changed revision, and retries failures', async () => {
  const pending = [];
  const responses = ['1', '1', new Error('offline'), '2'];
  let reloads = 0;
  vm.runInNewContext(script, {
    document: {hidden: false},
    location: {reload() { reloads++; }},
    setTimeout(callback) { pending.push(callback); },
    async fetch(url, options) {
      assert.equal(url, '/__berlin/revision');
      assert.equal(options.cache, 'no-store');
      assert.equal(options.method, undefined); // Fetch defaults to GET, never HEAD.
      const value = responses.shift();
      if (value instanceof Error) throw value;
      return {ok: true, text: async () => value};
    },
  });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(reloads, 0);
  for (let step = 0; step < 3; step++) await pending.shift()();
  assert.equal(reloads, 1);
  assert.equal(pending.length, 1);
});
