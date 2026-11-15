const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const html = fs.readFileSync(path.join(__dirname, '../../cli/tasks/announcement/linkedin/callback.html'), 'utf8');
const script = html.split('<script>')[1].split('</script>')[0];

function page(href) {
  const events = [], blobs = [];
  const status = {textContent: ''};
  const button = {disabled: true, addEventListener(type, handler) { assert.equal(type, 'click'); this.click = handler; }};
  class LocalURL extends URL {
    static createObjectURL(blob) { blobs.push(blob); return 'blob:local-response'; }
    static revokeObjectURL(url) { assert.equal(url, 'blob:local-response'); }
  }
  const location = new URL(href);
  vm.runInNewContext(script, {
    URL: LocalURL, Blob, setTimeout: fn => fn(),
    window: {location, history: {replaceState(_state, _title, url) { events.push('clear-address'); location.href = new URL(url, location).href; }}},
    document: {
      getElementById: id => id === 'status' ? status : button,
      createElement(tag) { assert.equal(tag, 'a'); return {click() { events.push('download'); assert.equal(this.download, 'linkedin-callback.txt'); assert.equal(this.href, 'blob:local-response'); }}; },
    },
    // No fetch, XHR, cookies, storage or third-party services are available.
  });
  return {events, blobs, status, button, location};
}
test('authorization response is cleared from the address and only saved on user action', async () => {
  const original = 'https://example.test/callback?code=temporary-secret&state=random';
  const result = page(original);
  assert.equal(result.location.href, 'https://example.test/callback');
  assert.deepEqual(result.events, ['clear-address']);
  assert.equal(result.blobs.length, 0);
  assert.equal(result.button.disabled, false);
  assert.ok(!result.status.textContent.includes('temporary-secret'));
  result.button.click();
  assert.deepEqual(result.events, ['clear-address', 'download']);
  assert.equal(await result.blobs[0].text(), original);
});
test('declined or missing authorization never enables a download', () => {
  for (const suffix of ['', '?error=declined&state=random', '?code=temporary-secret']) {
    const result = page('https://example.test/callback' + suffix);
    assert.equal(result.button.disabled, true);
    assert.equal(result.blobs.length, 0);
    assert.equal(result.button.click, undefined);
  }
});
