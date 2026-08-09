import { spawn } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import { extname, resolve, sep } from 'node:path';
import { acquireManagedTransientDirectory } from './managed-transient-directory.mjs';

const options = parseArgs(process.argv.slice(2));
const root = resolve(options.root);
const benchmarkEntry = resolve(root, 'index.html');
if (!fs.existsSync(benchmarkEntry) || !fs.statSync(benchmarkEntry).isFile()) {
  throw new Error(
    `benchmark root must contain a built index.html entrypoint: ${benchmarkEntry}`
  );
}
const timeoutMs = positiveInteger(options.timeout ?? '3600000', 'timeout');
const cacheBase = process.platform === 'win32'
  ? process.env.LOCALAPPDATA || process.env.TEMP || resolve(process.env.USERPROFILE || '.', 'AppData', 'Local')
  : process.env.XDG_CACHE_HOME || resolve(process.env.HOME || '.', '.cache');
const profileLease = await acquireManagedTransientDirectory(
  resolve(cacheBase, 'Clearra', 'benchmark-runtime', 'browser-profile')
);
const profile = profileLease.path;
let browser = null;
let server = null;
let timeout = null;
let lastProgress = null;
const progressSamples = [];
let processMemoryProbe = null;

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
      if (options.command) url.searchParams.set('command', options.command);
      if (options['prewarm-workers']) {
        url.searchParams.set('prewarmWorkers', options['prewarm-workers']);
      }
      if (options['prewarm-gpu']) url.searchParams.set('prewarmGpu', options['prewarm-gpu']);
      if (options['runtime-prewarm-workers']) {
        url.searchParams.set('runtimePrewarmWorkers', options['runtime-prewarm-workers']);
      }
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
      processMemoryProbe = startProcessMemoryProbe(browser.pid, profile, options);
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
  const processMemory = await finishProcessMemoryProbe(processMemoryProbe);
  processMemoryProbe = null;
  outcome.result.browser_process_tree_peak_working_set_bytes = processMemory.peakBytes;
  outcome.result.browser_process_tree_memory_probe = processMemory.status;
  outcome.result.browser_process_tree_memory_sample_interval_ms = processMemory.intervalMs;
  await stopBrowser(browser, profile);
  browser = null;
  await closeServer(server);
  server = null;
  console.log(JSON.stringify(outcome.result, null, 2));
} finally {
  clearTimeout(timeout);
  await finishProcessMemoryProbe(processMemoryProbe);
  await stopBrowser(browser, profile);
  await closeServer(server);
  try {
    await removeBrowserProfile(profile);
  } finally {
    await profileLease.release({ remove: false });
  }
}

function startProcessMemoryProbe(rootPid, profilePath, values) {
  if (values['memory-probe'] !== 'true') return null;
  if (process.platform !== 'win32') {
    return { unsupported: true, intervalMs: null };
  }
  const intervalMs = positiveInteger(
    values['memory-sample-interval'] ?? '250',
    'memory-sample-interval'
  );
  const outputPath = resolve(profilePath, 'process-tree-peak-working-set.txt');
  const stopPath = resolve(profilePath, 'process-tree-memory-probe.stop');
  const source = String.raw`
$ErrorActionPreference = 'Stop'
$rootProcessId = [uint32]$env:CLEARRA_MEMORY_PROBE_ROOT_PID
$stopPath = $env:CLEARRA_MEMORY_PROBE_STOP_PATH
$outputPath = $env:CLEARRA_MEMORY_PROBE_OUTPUT_PATH
$intervalMs = [int]$env:CLEARRA_MEMORY_PROBE_INTERVAL_MS
$peakBytes = [long]0
try {
  while (-not (Test-Path -LiteralPath $stopPath)) {
    $rows = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,WorkingSetSize)
    $ids = [System.Collections.Generic.HashSet[uint32]]::new()
    [void]$ids.Add($rootProcessId)
    do {
      $changed = $false
      foreach ($row in $rows) {
        if ($ids.Contains([uint32]$row.ParentProcessId) -and $ids.Add([uint32]$row.ProcessId)) {
          $changed = $true
        }
      }
    } while ($changed)
    $workingSetBytes = [long]0
    foreach ($row in $rows) {
      if ($ids.Contains([uint32]$row.ProcessId)) {
        $workingSetBytes += [long]$row.WorkingSetSize
      }
    }
    if ($workingSetBytes -gt $peakBytes) { $peakBytes = $workingSetBytes }
    Start-Sleep -Milliseconds $intervalMs
  }
} finally {
  [System.IO.File]::WriteAllText($outputPath, [string]$peakBytes)
}
`;
  const encoded = Buffer.from(source, 'utf16le').toString('base64');
  const child = spawn('powershell.exe', [
    '-NoLogo',
    '-NoProfile',
    '-NonInteractive',
    '-EncodedCommand',
    encoded,
  ], {
    stdio: 'ignore',
    windowsHide: true,
    env: {
      ...process.env,
      CLEARRA_MEMORY_PROBE_ROOT_PID: String(rootPid),
      CLEARRA_MEMORY_PROBE_STOP_PATH: stopPath,
      CLEARRA_MEMORY_PROBE_OUTPUT_PATH: outputPath,
      CLEARRA_MEMORY_PROBE_INTERVAL_MS: String(intervalMs),
    },
  });
  return { child, outputPath, stopPath, intervalMs };
}

async function finishProcessMemoryProbe(probe) {
  if (!probe) return { peakBytes: null, status: 'not-requested', intervalMs: null };
  if (probe.unsupported) {
    return { peakBytes: null, status: 'unsupported-platform', intervalMs: null };
  }
  await fs.promises.writeFile(probe.stopPath, '', 'utf8');
  const exited = probe.child.exitCode === null
    ? new Promise((resolveExit) => probe.child.once('exit', resolveExit))
    : Promise.resolve(probe.child.exitCode);
  const completed = await Promise.race([
    exited.then(() => true),
    new Promise((resolveWait) => setTimeout(() => resolveWait(false), 5_000)),
  ]);
  if (!completed) probe.child.kill('SIGKILL');
  let peakBytes = null;
  try {
    const parsed = Number.parseInt(await fs.promises.readFile(probe.outputPath, 'utf8'), 10);
    if (Number.isSafeInteger(parsed) && parsed > 0) peakBytes = parsed;
  } catch {
    // A missing sample is explicit in the returned probe status.
  }
  return {
    peakBytes,
    status: peakBytes === null ? 'no-sample' : 'sampled-windows-process-tree-working-set',
    intervalMs: probe.intervalMs,
  };
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
  if (!parsed.root || (!parsed.command && !parsed['prewarm-workers'])) {
    throw new Error('--root and either --command or --prewarm-workers are required');
  }
  return parsed;
}

function positiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}
