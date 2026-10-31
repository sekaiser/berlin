#!/usr/bin/env python3
"""Prepare synthetic browser-review pages; does not open a browser or run servers."""
import argparse
import json
import os
from pathlib import Path
import shutil
import shlex
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--base-port', type=int, default=8093)
args = parser.parse_args()
if not 1024 <= args.base_port <= 65530:
    parser.error('--base-port must leave room for six ports between 1024 and 65535')
repo = Path(__file__).resolve().parent.parent
binary = repo / 'target/debug/bln'
if not binary.is_file():
    parser.error('Build bln first: cargo build --offline --locked --bin bln')
packet = Path(tempfile.mkdtemp(prefix='berlin-visual-review-'))
url = lambda offset: f'http://127.0.0.1:{args.base_port + offset}'
env = {**os.environ, 'BERLIN_THEME_DIR': str(repo / 'themes'), 'BERLIN_TEST_BINARY': str(binary)}
projects = {}
for name in ['default', 'alternate']:
    project = packet / name
    shutil.copytree(repo / 'support/fixtures/unified-resources', project)
    config = project / 'berlin.pipeline.rhai'
    text = config.read_text().replace('https://example.test/notebook', url(0))
    text = text.replace('https://example.test/portfolio', url(1 if name == 'default' else 2))
    text = text.replace('https://example.test/excerpts', url(3))
    config.write_text(text)
    projects[name] = project
alternate = repo / 'support/fixtures/composable-publications/alternate'
for directory in ['pages', 'styles']:
    shutil.copytree(alternate / directory, projects['alternate'] / directory)
for name, pipelines in [('default', ['site', 'portfolio', 'excerpts']), ('alternate', ['portfolio'])]:
    for pipeline in pipelines:
        subprocess.run([str(binary), 'build', '--pipeline', pipeline],
                       env={**env, 'BERLIN_DIR': str(projects[name])}, check=True)
entries = [
    {'name': 'Notebook and reading list', 'root': str(projects['default'] / '_site'), 'url': url(0)},
    {'name': 'Default mixed-resource portfolio', 'root': str(projects['default'] / '_portfolio'), 'url': url(1)},
    {'name': 'Alternate portfolio presentation', 'root': str(projects['alternate'] / '_portfolio'), 'url': url(2)},
    {'name': 'Repeated excerpts', 'root': str(projects['default'] / '_excerpts'), 'url': url(3)},
]
for variant, offset in [('stress', 4), ('empty', 5)]:
    script = """const {buildSpecimen}=require(process.argv[1]);
const s=buildSpecimen(process.argv[2],process.argv[3]);
process.stdout.write(JSON.stringify({root:s.root,output:s.output}));"""
    result = subprocess.run(['node', '-e', script, str(repo / 'themes/notebook/tests/specimen.cjs'),
                             variant, url(offset)], env=env, check=True, capture_output=True, text=True)
    specimen = json.loads(result.stdout)
    entries.append({'name': f'{variant.title()} notebook', 'root': specimen['output'], 'url': url(offset)})
manifest = {
    'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
    'purpose': 'Synthetic manual review only; no browser checks performed',
    'pages': entries,
}
(packet / 'review.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(f'Visual review packet: {packet}')
print('Serve each output separately (one terminal per command):')
for entry in entries:
    port = entry['url'].rsplit(':', 1)[1]
    print(f"  {entry['name']}: {entry['url']}/")
    print(f"  python3 -m http.server {port} --bind 127.0.0.1 --directory {shlex.quote(entry['root'])}")
print('No servers or browsers were started. Specimen directories remain available for manual review.')
