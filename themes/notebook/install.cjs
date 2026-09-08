// Copy a vendored theme into a publishing project. No npm dependencies.
const fs = require('node:fs');
const path = require('node:path');

function files(root, relative = '') {
  return fs.readdirSync(path.join(root, relative), {withFileTypes: true}).flatMap(entry => {
    const name = path.join(relative, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`Theme contains a symlink: ${name}`);
    return entry.isDirectory() ? files(root, name) : [name];
  });
}

function rejectSymlinks(destination) {
  for (let item = destination; ; item = path.dirname(item)) {
    if (fs.lstatSync(item, {throwIfNoEntry: false})?.isSymbolicLink()) {
      throw new Error(`Refusing a symlink destination: ${item}`);
    }
    if (item === path.dirname(item)) return;
  }
}

function install(destination, {update = false, check = false} = {}) {
  // Resolve platform aliases such as macOS /tmp before checking child paths.
  let ancestor = path.resolve(destination);
  const suffix = [];
  while (!fs.existsSync(ancestor)) {
    suffix.unshift(path.basename(ancestor));
    ancestor = path.dirname(ancestor);
  }
  const root = path.join(fs.realpathSync(ancestor), ...suffix);
  const repository = path.resolve(__dirname, '../..');
  if (root === repository || root === path.parse(root).root ||
      root === __dirname || root.startsWith(__dirname + path.sep)) {
    throw new Error('Choose a separate publishing project, not the repository or theme itself');
  }
  rejectSymlinks(root);
  const writes = [];
  const differences = [];
  for (const family of ['shared', 'defaults']) {
    for (const name of files(path.join(__dirname, family))) {
      const target = path.join(root, name);
      rejectSymlinks(target);
      const exists = fs.existsSync(target);
      if (exists && !fs.statSync(target).isFile()) throw new Error(`Expected a file: ${target}`);
      // Defaults become project-owned after the first installation.
      if (family === 'defaults' && exists) continue;
      const contents = fs.readFileSync(path.join(__dirname, family, name));
      if (exists && fs.readFileSync(target).equals(contents)) continue;
      differences.push(name);
      if (exists && !update && !check) {
        throw new Error(`Shared theme file differs: ${name}. Review it before using --update.`);
      }
      writes.push({target, contents});
    }
  }
  if (check) return differences;
  // Preflight above avoids changing anything when a destination conflict exists.
  for (const {target, contents} of writes) {
    fs.mkdirSync(path.dirname(target), {recursive: true});
    fs.writeFileSync(target, contents);
  }
  return differences;
}

if (require.main === module) {
  try {
    const [destination, ...flags] = process.argv.slice(2);
    if (!destination || flags.some(flag => !['--update', '--check'].includes(flag))) {
      throw new Error('Usage: node themes/notebook/install.cjs PROJECT [--update | --check]');
    }
    const check = flags.includes('--check');
    const changed = install(destination, {update: flags.includes('--update'), check});
    console.log(`${check ? 'Different or missing' : 'Installed'} theme files: ${changed.length}`);
    if (check && changed.length) { console.log(changed.join('\n')); process.exitCode = 1; }
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}

module.exports = {install};
