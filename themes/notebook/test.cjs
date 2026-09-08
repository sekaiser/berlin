const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {spawnSync} = require('node:child_process');
const {install} = require('./install.cjs');
const repository = path.resolve(__dirname, '../..');

function temporary(fn) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'berlin-theme-test-'));
  try { fn(root); } finally { fs.rmSync(root, {recursive: true, force: true}); }
}

test('installation is idempotent and preserves project-owned content on update', () => temporary(root => {
  assert.ok(install(root).length > 0);
  assert.deepEqual(install(root), []);
  const personal = path.join(root, 'pages/index.tera');
  fs.writeFileSync(personal, 'Personal introduction');
  fs.writeFileSync(path.join(root, 'styles/entries/project.css'), '/* personal artwork */');
  const shared = path.join(root, 'styles/entries/notebook.css');
  fs.writeFileSync(shared, '/* reviewed local change */');
  assert.throws(() => install(root), /Shared theme file differs/);
  assert.equal(fs.readFileSync(shared, 'utf8'), '/* reviewed local change */');
  assert.deepEqual(install(root, {check: true}), ['styles/entries/notebook.css']);
  install(root, {update: true});
  assert.equal(fs.readFileSync(personal, 'utf8'), 'Personal introduction');
  assert.equal(fs.readFileSync(path.join(root, 'styles/entries/project.css'), 'utf8'), '/* personal artwork */');
  assert.deepEqual(install(root, {check: true}), []);
}));

test('preflight refuses file conflicts and nested symlinks before writing', () => temporary(root => {
  fs.mkdirSync(path.join(root, 'styles'));
  fs.writeFileSync(path.join(root, 'styles/base.css'), 'Keep this');
  assert.throws(() => install(root), /Shared theme file differs/);
  assert.ok(!fs.existsSync(path.join(root, 'pages')));
  fs.unlinkSync(path.join(root, 'styles/base.css'));
  fs.symlinkSync(path.join(root, 'missing'), path.join(root, 'styles/base.css'));
  assert.throws(() => install(root, {update: true}), /symlink/);
  assert.ok(!fs.existsSync(path.join(root, 'missing')));
  assert.throws(() => install(repository), /separate publishing project/);
}));

test('bundled notices and fonts accompany the theme without UnoCSS', () => temporary(root => {
  install(root);
  for (const name of ['LICENSE', 'NOTICE']) {
    assert.equal(fs.readFileSync(path.join(root, `static/licenses/berlin/${name}.txt`), 'utf8'),
      fs.readFileSync(path.join(repository, name), 'utf8').trimEnd() + '\n');
  }
  for (const name of ['regular.woff2', 'italic.woff2', 'OFL.txt']) {
    assert.ok(fs.statSync(path.join(root, 'static/fonts/fraunces', name)).size > 0);
  }
  assert.ok(!fs.existsSync(path.join(root, 'package.json')));
  assert.ok(!fs.existsSync(path.join(root, 'static/css/uno.css')));
}));

test('clean example builds real page families, links, controls and palette at both URL bases', () => {
  for (const url of ['https://notebook.example', 'https://notebook.example/notebook']) temporary(root => {
    fs.cpSync(path.join(__dirname, 'example'), root, {recursive: true});
    const note = path.join(root, 'content/notes/components.md');
    fs.writeFileSync(note, fs.readFileSync(note, 'utf8').replace('---', [
      '---', 'preview:', '  source: /static/pics/study/engineering-desk.webp',
      '  alt: \'A "desk" <not markup>\'', '  width: 1254', '  height: 1254',
    ].join('\n')));
    const pipeline = path.join(root, 'berlin.pipeline.rhai');
    fs.writeFileSync(pipeline, fs.readFileSync(pipeline, 'utf8')
      .replace('http://localhost:8082', url)
      .replace('"themes/notebook"', JSON.stringify(__dirname)));
    const build = spawnSync(path.join(repository, 'target/debug/bln'), ['build', '--pipeline', 'site'],
      {env: {...process.env, BERLIN_DIR: root}, encoding: 'utf8'});
    assert.equal(build.status, 0, build.stderr);
    assert.ok(!fs.existsSync(path.join(root, 'pages')), 'build must not install templates');
    assert.ok(!fs.existsSync(path.join(root, 'styles/entries/notebook.css')), 'build must not install theme styles');
    const read = file => fs.readFileSync(path.join(root, '_site', file), 'utf8').replaceAll('&#x2F;', '/');
    for (const page of ['index.html', 'notes.html', 'feed.html', 'about.html', 'search.html',
      'notes/component-specimens.html', 'notes/inspecting-the-theme.html', 'tags/design.html', 'tags/systems.html']) {
      const html = read(page);
      assert.doesNotMatch(html, /Sebastian|Processing JSON|org-mode-publishing-showcase|uno\.css|live\.js/);
      assert.ok(html.includes(`href="${url}/${page === 'index.html' ? '' : page}"`), page);
      assert.match(html, /class="skip-link"/);
      for (const [, href] of html.matchAll(/(?:href|src)="([^"]+)"/g)) {
        const target = new URL(href, `${url}/${page}`);
        const base = new URL(url + '/');
        if (target.origin !== base.origin) continue;
        assert.ok(target.pathname.startsWith(base.pathname), `URL escapes site prefix: ${href}`);
        const relative = target.pathname.slice(base.pathname.length) || 'index.html';
        const destination = path.join(root, '_site', relative);
        assert.ok(fs.existsSync(destination), `${page}: ${href}`);
        if (target.hash && relative.endsWith('.html')) assert.ok(
          fs.readFileSync(destination, 'utf8').includes(`id="${decodeURIComponent(target.hash.slice(1))}"`), href);
      }
    }
    const article = read('notes/component-specimens.html');
    assert.match(article, /class="study-page"/);
    assert.match(article, /class="article-backlinks"/);
    assert.match(article, /class="code-copy"/);
    assert.match(article, /id="palette"/);
    assert.match(read('feed.html'), /reading-tag-options/);
    const bookmarks = [...read('feed.html').matchAll(/<article class="journal-bookmark">([\s\S]*?)<\/article>/g)].map(match => match[1]);
    assert.equal(bookmarks.filter(html => html.includes('data-tag="design"') && html.includes('data-tag="systems"')).length, 1);
    assert.match(read('index.html'), /notebook-home/);
    for (const page of ['index.html', 'notes.html', 'tags/design.html']) {
      const entries = [...read(page).matchAll(/<article class="journal-entry">([\s\S]*?)<\/article>/g)]
        .map(match => match[1]);
      const illustrated = entries.filter(entry => entry.includes('journal-entry-visual'));
      assert.equal(illustrated.length, 1, page);
      assert.ok(illustrated[0].includes(`src="${url}/static/pics/study/engineering-desk.webp"`));
      const alt = illustrated[0].match(/alt="([^"]*)"/)[1];
      assert.equal(alt.replaceAll('&quot;', '"').replaceAll('&#x22;', '"'), 'A "desk" &lt;not markup&gt;');
      assert.ok(illustrated[0].includes('notes/component-specimens.html'));
    }
    assert.match(read('index.html'), /class="journal-desk"/);
    assert.match(read('index.html'), /engineering-desk-320\.webp 320w/);
    for (const name of ['engineering-desk.png', 'engineering-desk.webp', 'engineering-desk-640.webp', 'engineering-desk-320.webp']) {
      assert.ok(fs.statSync(path.join(root, '_site/static/pics/study', name)).size > 0);
    }
    assert.ok(!fs.existsSync(path.join(root, 'static/pics/study')), 'build must not install desk assets in the project');
    assert.match(read('assets/css/notebook.css'), /--ink:#273b32/);
    assert.match(read('assets/css/notebook.css'), /--accent:#94492f/);
    assert.match(read('assets/css/vendor.css'), /body\{margin:0\}/);
    assert.match(read('assets/css/vendor.css'), /\.pl-c\{color:#6a737d\}/);
    assert.match(read('static/licenses/notebook-vendor.txt'), /Nicolas Gallagher and Jonathan Neal/);
    assert.match(read('static/licenses/notebook-vendor.txt'), /GitHub, Inc\./);
    for (const page of ['index.html', 'feed.html', 'notes/component-specimens.html']) {
      const html = read(page);
      assert.ok(html.indexOf('/assets/css/vendor.css') >= 0);
      assert.ok(html.indexOf('/assets/css/vendor.css') < html.indexOf('/assets/css/notebook.css'));
    }
    assert.ok(!fs.existsSync(path.join(root, 'styles/entries/vendor.css')), 'vendor styles belong to the theme');
    assert.ok(!fs.existsSync(path.join(root, '_site/css')));
    assert.ok(!fs.existsSync(path.join(root, '_site/static/css')));
    assert.ok(!fs.existsSync(path.join(root, '_site/assets/css/tokens.css')));
    const index = JSON.parse(read('search/index.json'));
    assert.equal(index.documents.length, 2);
    assert.ok(index.documents.some(document => document.path === 'notes/component-specimens.html'));
  });
});
