import { spawn } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import { extname, resolve, sep } from 'node:path';

const options = parseArgs(process.argv.slice(2));
const root = resolve(options.root);
const timeoutMs = positiveInteger(options.timeout ?? '3600000', 'timeout');
const profile = fs.mkdtempSync(resolve(os.tmpdir(), 'clearra-browser-benchmark-'));
let browser = null;
let server = null;
let timeout = null;
let lastProgress = null;
const progressSamples = [];

try {
  const outcome = await new Promise((resolveResult, rejectResult) => {
    server = http.createServer((request, response) => {
      if (request.method === 'POST' && request.url === '/__progress') {
        collectBody(request).then((body) => {
          lastProgress = JSON.parse(body);
          progressSamples.push(lastProgress);
          response.writeHead(204, benchmarkHeaders());
          response.end();
        }, rejectResult);
        return;
      }
      if (request.method === 'POST' && request.url === '/__result') {
        collectBody(request).then((body) => {
          response.writeHead(204, benchmarkHeaders());
          response.end();
          resolveResult({
            result: {
              ...JSON.parse(body),
              benchmark_progress_samples: progressSamples
            }
          });
        }, rejectResult);
        return;
      }
      serveStatic(root, request, response);
    });
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      if (!address || typeof address === 'string') {
        rejectResult(new Error('benchmark server did not expose a TCP port'));
        return;
      }
      const url = new URL(`http://127.0.0.1:${address.port}/`);
      url.searchParams.set('command', options.command);
      browser = spawn(resolveBrowser(options.browser), [
        `--user-data-dir=${profile}`,
        '--headless=new',
        '--remote-debugging-port=0',
        '--no-first-run',
        '--no-default-browser-check',
        '--disable-background-networking',
        '--disable-component-update',
        '--disable-sync',
        '--metrics-recording-only',
        url.href,
      ], { stdio: ['ignore', 'ignore', 'pipe'], windowsHide: true });
      let browserError = '';
      browser.stderr.on('data', (chunk) => { browserError += chunk.toString(); });
      browser.once('error', rejectResult);
      browser.once('exit', (code) => {
        if (code && code !== 0) rejectResult(new Error(`browser exited ${code}: ${browserError}`));
      });
      timeout = setTimeout(() => resolveResult({ result: {
        surface: 'browser-wasm-product-worker',
        timed_out: true,
        timeout_ms: timeoutMs,
        command: options.command,
        last_progress: lastProgress,
        benchmark_progress_samples: progressSamples
      } }), timeoutMs);
    });
  });
  clearTimeout(timeout);
  await stopBrowser(browser, profile);
  browser = null;
  await closeServer(server);
  server = null;
  console.log(JSON.stringify(outcome.result, null, 2));
} finally {
  clearTimeout(timeout);
  await stopBrowser(browser, profile);
  await closeServer(server);
  await removeBrowserProfile(profile);
}

async function closeServer(activeServer) {
  if (!activeServer?.listening) return;
  await new Promise((resolveClose) => activeServer.close(resolveClose));
}

async function stopBrowser(activeBrowser, profilePath) {
  if (!activeBrowser || activeBrowser.exitCode !== null) return;
  const exited = new Promise((resolveExit) => activeBrowser.once('exit', resolveExit));
  await requestBrowserClose(profilePath);
  const closed = await Promise.race([
    exited.then(() => true),
    new Promise((resolveWait) => setTimeout(() => resolveWait(false), 5000))
  ]);
  if (closed) return;
  activeBrowser.kill('SIGKILL');
  await Promise.race([
    exited,
    new Promise((resolveWait) => setTimeout(resolveWait, 5000))
  ]);
}

async function removeBrowserProfile(profilePath) {
  const deadline = Date.now() + 30_000;
  for (;;) {
    try {
      await fs.promises.rm(profilePath, { recursive: true, force: true });
      return;
    } catch (error) {
      if (!['EBUSY', 'ENOTEMPTY', 'EPERM'].includes(error?.code) || Date.now() >= deadline) {
        throw error;
      }
      await new Promise((resolveWait) => setTimeout(resolveWait, 250));
    }
  }
}

async function requestBrowserClose(profilePath) {
  const endpointFile = resolve(profilePath, 'DevToolsActivePort');
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (fs.existsSync(endpointFile)) {
      const [port, endpoint] = fs.readFileSync(endpointFile, 'utf8').trim().split(/\r?\n/);
      if (port && endpoint) {
        const socket = new WebSocket(`ws://127.0.0.1:${port}${endpoint}`);
        await new Promise((resolveSocket, rejectSocket) => {
          socket.addEventListener('open', () => {
            socket.send(JSON.stringify({ id: 1, method: 'Browser.close' }));
            resolveSocket();
          }, { once: true });
          socket.addEventListener('error', rejectSocket, { once: true });
        });
        return;
      }
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 50));
  }
}

function serveStatic(root, request, response) {
  const pathname = new URL(request.url ?? '/', 'http://localhost').pathname;
  const relative = pathname === '/' ? 'index.html' : pathname.slice(1);
  const path = resolve(root, relative);
  if (path !== root && !path.startsWith(`${root}${sep}`)) {
    response.writeHead(403, benchmarkHeaders());
    response.end('forbidden');
    return;
  }
  fs.readFile(path, (error, bytes) => {
    if (error) {
      response.writeHead(404, benchmarkHeaders());
      response.end('not found');
      return;
    }
    response.writeHead(200, {
      ...benchmarkHeaders(),
      'content-type': mimeType(path),
      'cache-control': 'no-store'
    });
    response.end(bytes);
  });
}

function benchmarkHeaders() {
  return {
    'cross-origin-opener-policy': 'same-origin',
    'cross-origin-embedder-policy': 'require-corp'
  };
}

function collectBody(request) {
  return new Promise((resolveBody, rejectBody) => {
    const chunks = [];
    let length = 0;
    request.on('data', (chunk) => {
      length += chunk.length;
      if (length > 16 * 1024 * 1024) {
        rejectBody(new Error('benchmark result exceeds 16 MiB'));
        request.destroy();
      } else {
        chunks.push(chunk);
      }
    });
    request.on('end', () => resolveBody(Buffer.concat(chunks).toString('utf8')));
    request.on('error', rejectBody);
  });
}

function resolveBrowser(requested) {
  const candidates = [
    requested,
    process.env.CLEARRA_BROWSER_PATH,
    'C:/Program Files/Google/Chrome/Application/chrome.exe',
    'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'
  ].filter(Boolean);
  const browserPath = candidates.find((candidate) => fs.existsSync(candidate));
  if (!browserPath) throw new Error('Chrome or Edge executable was not found');
  return browserPath;
}

function mimeType(path) {
  return ({
    '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8',
    '.css': 'text/css; charset=utf-8', '.json': 'application/json; charset=utf-8',
    '.wasm': 'application/wasm'
  })[extname(path)] ?? 'application/octet-stream';
}

function parseArgs(args) {
  const parsed = {};
  for (let index = 0; index < args.length; index += 2) {
    const key = args[index];
    const value = args[index + 1];
    if (!key?.startsWith('--') || value === undefined) throw new Error(`invalid argument: ${key ?? ''}`);
    parsed[key.slice(2)] = value;
  }
  if (!parsed.root || !parsed.command) throw new Error('--root and --command are required');
  return parsed;
}

function positiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}
