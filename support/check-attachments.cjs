// Verify real Org exports publish images and downloads with matching URLs.
const fs = require('node:fs');
const path = require('node:path');
const {createHash} = require('node:crypto');
const assert = require('node:assert/strict');
const root = process.argv[2];
const html = fs.readFileSync(path.join(root, '_site/notes/example-article.html'), 'utf8');
for (const name of ['diagram.svg', 'example.csv']) {
  const source = fs.readFileSync(path.join(root, 'data', name));
  const hash = createHash('sha256').update(source).digest('hex');
  const relative = `attachments/${hash}/${name}`;
  assert.deepEqual(fs.readFileSync(path.join(root, '.berlin/generated/org/static', relative)), source);
  assert.deepEqual(fs.readFileSync(path.join(root, '_site', relative)), source);
  assert.ok(html.includes(`/${relative}`), `Missing rendered attachment URL: ${name}`);
  assert.ok(!html.includes(`/static/${relative}`));
}
console.log('Exported image and download URLs resolve to identical published bytes.');
