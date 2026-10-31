// Builds isolated, synthetic specimens through Berlin's actual theme pipeline.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {spawnSync} = require('node:child_process');
const theme = path.resolve(__dirname, '..');
const repository = path.resolve(theme, '../..');

function buildSpecimen(variant, url = 'https://notebook.example/notebook') {
  if (!['stress', 'empty'].includes(variant)) throw new Error(`Unknown specimen: ${variant}`);
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'berlin-theme-specimen-'));
  const dispose = () => fs.rmSync(root, {recursive: true, force: true});
  try {
    const example = path.join(theme, 'example');
    fs.cpSync(example, root, {recursive: true,
      filter: source => variant !== 'empty' || source !== path.join(example, 'content')});
    if (variant === 'stress') {
      fs.cpSync(path.join(__dirname, 'fixtures/stress'), root, {recursive: true});
    } else {
      fs.mkdirSync(path.join(root, 'content/notes'), {recursive: true});
      fs.writeFileSync(path.join(root, 'data/resources.yaml'), 'schema_version: 1\nresources: []\n');
    }
    const config = path.join(root, 'berlin.pipeline.rhai');
    fs.writeFileSync(config, fs.readFileSync(config, 'utf8')
      .replace('"themes/notebook"', JSON.stringify(theme))
      .replaceAll('http://localhost:8082', url)
      .replace(variant === 'empty' ? 'documents(load_markdown("content/notes/*.md")),' : '__unused__', ''));
    const result = spawnSync(process.env.BERLIN_TEST_BINARY || path.join(repository, 'target/debug/bln'),
      ['build', '--pipeline', 'site'], {env: {...process.env, BERLIN_DIR: root}, encoding: 'utf8'});
    if (result.status !== 0) throw new Error(result.error?.message || result.stderr || `Build exited ${result.status}`);
    return {root, output: path.join(root, '_site'), url, dispose};
  } catch (error) { dispose(); throw error; }
}

module.exports = {buildSpecimen};
