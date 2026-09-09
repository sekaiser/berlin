// A real, loopback-only preview of isolated build output, including native downloads.
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const {buildSpecimen} = require('./specimen.cjs');

const mime = {'.html':'text/html', '.css':'text/css', '.js':'text/javascript', '.json':'application/json', '.svg':'image/svg+xml', '.webp':'image/webp', '.png':'image/png', '.woff2':'font/woff2', '.org':'text/plain; charset=utf-8'};

async function previewSpecimen(variant) {
  let project;
  const prefix = '/notebook/';
  const server = http.createServer((request, response) => {
    if (request.method !== 'GET') {response.writeHead(405).end(); return;}
    if (!project) {response.writeHead(503).end(); return;}
    try {
      const pathname = new URL(request.url, 'http://localhost').pathname;
      if (!pathname.startsWith(prefix)) {response.writeHead(404).end(); return;}
      const relative = decodeURIComponent(pathname.slice(prefix.length)) || 'index.html';
      const file = path.resolve(project.output, relative);
      if (!file.startsWith(project.output + path.sep) || !fs.statSync(file).isFile()) {
        response.writeHead(404).end(); return;
      }
      // Tiny, local fixture files; read before writing headers so failures remain HTTP errors.
      const body = fs.readFileSync(file);
      response.writeHead(200, {'Content-Type':mime[path.extname(file)] || 'application/octet-stream', 'Content-Length':body.length});
      response.end(body);
    } catch {response.writeHead(404).end();}
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const close = () => new Promise((resolve, reject) => {
    server.close(error => error ? reject(error) : resolve());
    // A canceled browser download must not keep the test's HTTP preview alive.
    server.closeAllConnections();
  });
  try {
    project = buildSpecimen(variant, `http://127.0.0.1:${server.address().port}/notebook`);
    return {...project, variant, async dispose() {await close(); project.dispose();}};
  } catch (error) {await close(); throw error;}
}

module.exports = {previewSpecimen};
