import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
const page = await readFile(new URL('../tests/fixtures/dashboard.html', import.meta.url));
const server = createServer((request, response) => {
  if (request.url !== '/' && request.url !== '/dashboard') { response.writeHead(404).end('Not found'); return; }
  response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' }).end(page);
});
server.listen(4174, '127.0.0.1', () => console.log(`Fixture ready http://127.0.0.1:4174 PID=${process.pid}`));
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => server.close(() => process.exit(0)));
