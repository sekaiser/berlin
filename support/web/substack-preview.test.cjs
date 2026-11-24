const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const template = fs.readFileSync(path.join(__dirname, '../../cli/tasks/announcement/substack/preview.html'), 'utf8');
const script = template.split('<script>')[1].split('</script>')[0];

function page(mode) {
  const body = {innerHTML:'<p>Article <strong>body</strong></p>', innerText:'Article body'};
  const status = {textContent:''};
  const button = {addEventListener(type, handler) { assert.equal(type, 'click'); this.click = handler; }};
  const events = [], items = [];
  const selection = {removeAllRanges() { events.push('clear'); }, addRange() { events.push('select'); }};
  class ClipboardItem { constructor(data) { this.data = data; } }
  vm.runInNewContext(script, {
    Blob, ClipboardItem,
    navigator: mode === 'rich' ? {clipboard:{async write(value) {items.push(...value);}}} : {},
    window:{getSelection() { return selection; }},
    document:{
      getElementById(id) { return {'article-body':body, 'copy-status':status, 'copy-article':button}[id]; },
      createRange() { return {selectNodeContents(value) { assert.equal(value, body); }}; },
      execCommand(command) { assert.equal(command, 'copy'); return mode === 'fallback'; },
    },
    // No network, accounts, cookies or persistent storage are exposed.
  });
  return {body, status, button, events, items};
}
test('rich copy contains only the article, with HTML and plain-text alternatives', async () => {
  const p = page('rich');
  await p.button.click();
  assert.equal(p.items.length, 1);
  assert.equal(await p.items[0].data['text/html'].text(), p.body.innerHTML);
  assert.equal(await p.items[0].data['text/plain'].text(), p.body.innerText);
  assert.match(p.status.textContent, /Body copied/);
  assert.deepEqual(p.events, []);
});
test('local-file fallback copies the selection and reports success accurately', async () => {
  const p = page('fallback');
  await p.button.click();
  assert.deepEqual(p.events, ['clear', 'select', 'clear']);
  assert.match(p.status.textContent, /Body copied/);
});
test('unavailable clipboard leaves the body selected for an explicit manual copy', async () => {
  const p = page('manual');
  await p.button.click();
  assert.deepEqual(p.events, ['clear', 'select']);
  assert.match(p.status.textContent, /body is selected/);
  assert.doesNotMatch(p.status.textContent, /Body copied/);
});
