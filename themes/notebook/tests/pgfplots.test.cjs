const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {spawnSync} = require('node:child_process');
const {contrast} = require('./contrast.cjs');
const theme = path.resolve(__dirname, '..');
const repository = path.resolve(theme, '../..');
const authoring = path.join(theme, 'authoring/pgfplots');
const read = name => fs.readFileSync(path.join(authoring, name), 'utf8');

test('PGFPlots colors follow the notebook palette with readable ink and series', () => {
  const tokens = fs.readFileSync(path.join(theme, 'shared/styles/tokens.css'), 'utf8');
  const style = read('notebook-pgfplots.tex');
  const rgb = hex => `rgb(${hex.match(/../g).map(value => parseInt(value, 16)).join(',')})`;
  for (const [name, token] of Object.entries({notebookInk:'ink', notebookMoss:'moss-ink', notebookTerracotta:'accent', notebookSky:'sky-ink', notebookRule:'rule'})) {
    const color = style.match(new RegExp(`\\\\definecolor\\{${name}\\}\\{HTML\\}\\{([A-F0-9]+)\\}`))[1].toLowerCase();
    assert.equal(color, tokens.match(new RegExp(`--${token}: #([a-f0-9]+);`))[1]);
    if (name !== 'notebookRule') assert.ok(contrast(rgb(color), 'rgb(255,255,255)') >= 4.5);
  }
  assert.match(style, /solid,mark=\*/);
  assert.match(style, /dashed,mark=square\*/);
  assert.match(style, /densely dotted,mark=triangle\*/);
});

test('the notebook plot keeps the original fixture data and leaves data policy to the author', () => {
  assert.equal(read('values.csv'), fs.readFileSync(path.join(repository, 'support/fixtures/graphics/inputs/values.csv'), 'utf8'));
  assert.match(read('plot.tex'), /table\[x=x,y=y,col sep=comma\] \{values\.csv\}/);
  assert.doesNotMatch(read('notebook-pgfplots.tex'), /\b(?:xmin|xmax|ymin|ymax|xmode|ymode|domain|samples|smooth|filter)\s*=/);
  const svg = fs.readFileSync(path.join(theme, 'example/static/diagrams/notebook-plot.svg'), 'utf8');
  assert.equal([...svg.matchAll(/<path [^>]+fill='#435a3c'\/>/g)].length, 4);
  assert.doesNotMatch(svg, /<script|<foreignObject|(?:href|src)=['"]https?:|\/Users\/|\/private\//);
});

test('real TeX rendering is opt-in, supports both engines, and preserves author overrides', {skip:process.env.BERLIN_TEST_TEX !== '1'}, () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'berlin-notebook-tex-'));
  try {
    const inputs = path.join(root, 'inputs');
    fs.cpSync(path.join(repository, 'support/fixtures/graphics/inputs'), inputs, {recursive:true});
    fs.copyFileSync(path.join(authoring, 'notebook-pgfplots.tex'), path.join(inputs, 'notebook-pgfplots.tex'));
    function render(name, source, engine = 'pdflatex') {
      const file = path.join(root, `${name}.tex`);
      const output = path.join(root, `${name}.svg`);
      fs.writeFileSync(file, source);
      const result = spawnSync(process.env.BERLIN_TEST_BINARY || path.join(repository, 'target/debug/bln'),
        ['render-diagram', file, '--input-root', inputs, '--output', output, '--cache-root', root, '--engine', engine, '--allow-tex', '--no-cache'],
        {encoding:'utf8', timeout:120000});
      assert.equal(result.status, 0, result.stderr || result.error?.message);
      if (process.env.BERLIN_VALIDATION_DIR) {
        const artifacts = path.join(process.env.BERLIN_VALIDATION_DIR, 'pgfplots');
        fs.mkdirSync(artifacts, {recursive:true});
        fs.copyFileSync(output, path.join(artifacts, `${name}.svg`));
      }
      return fs.readFileSync(output, 'utf8');
    }
    const baseline = fs.readFileSync(path.join(repository, 'support/fixtures/graphics/plot.tex'), 'utf8');
    assert.equal(render('loaded-not-selected', baseline.replace('\\input{preamble.tex}', '\\input{preamble.tex}\n\\input{notebook-pgfplots.tex}')),
      render('baseline', baseline), 'Loading the style must not change an unselected plot');
    const example = read('plot.tex');
    const styled = render('notebook-pdftex', example);
    assert.equal(styled.trimEnd(), fs.readFileSync(path.join(theme, 'example/static/diagrams/notebook-plot.svg'), 'utf8').trimEnd());
    const lua = render('notebook-luatex', example, 'lualatex');
    assert.match(lua, /#435a3c/);
    const series = render('series-and-legend', example.replace('notebook axis,', 'notebook axis,legend pos=north west,').replace('\\end{axis}',
      '\\addplot coordinates {(0,3) (1,4) (2,5) (3,6)};\n\\addplot coordinates {(0,6) (1,5) (2,4) (3,3)};\n\\legend{Original,Test B,Test C}\n\\end{axis}'));
    for (const color of ['435a3c','94492f','34566f']) assert.ok(series.includes(`#${color}`));
    assert.match(series, /stroke-dasharray=/);
    const overridden = render('author-override', example.replace('\\addplot table', '\\addplot+[color=black,mark=x] table'));
    assert.doesNotMatch(overridden, /#435a3c/);
  } finally {fs.rmSync(root, {recursive:true, force:true});}
});
