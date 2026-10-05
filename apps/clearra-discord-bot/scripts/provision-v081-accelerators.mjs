// The accepted Cloud Run compute image, never the Discord Gateway, owns the
// immutable v0.8.1 accelerator data layer. The accepted CLI verifies the
// embedded signed catalog and every downloaded payload before activation.
import { execFileSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const PROFILES = Object.freeze(['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick']);
const PRODUCTS = Object.freeze([
  { command: 'legal-board', directory: 'legal-board', maximumBytes: 64 * 1024 * 1024,
    aggregateBytes: 320 * 1024 * 1024 },
  { command: 'reachability-pack', directory: 'conditioned-reachability', maximumBytes: 16 * 1024 * 1024,
    aggregateBytes: 80 * 1024 * 1024 },
]);
const DIGEST = /^[0-9a-f]{64}$/u;

export function requiresV081Accelerators(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:[-+][0-9A-Za-z.-]+)?$/u.exec(version ?? '');
  if (!match) throw new Error('Cloud compute product version is not semver');
  const [major, minor, patch] = match.slice(1).map(Number);
  if (![major, minor, patch].every(Number.isSafeInteger)) throw new Error('Cloud compute version is out of range');
  return major > 0 || minor > 8 || (minor === 8 && patch >= 1);
}

function invokeClearra(executable, arguments_, options) {
  const output = execFileSync(executable, arguments_, {
    encoding: 'utf8', timeout: options.timeoutMs, maxBuffer: 256 * 1024,
    stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true,
  });
  return JSON.parse(output);
}

function request(executable, product, action, profile, directory, invoke) {
  const arguments_ = [product.command, action, '--profile', profile];
  if (action !== 'check') arguments_.push('--directory', directory);
  arguments_.push('--format', 'json');
  return invoke(executable, arguments_, { timeoutMs: action === 'download' ? 1_800_000 : 30_000 });
}

function assertCatalog(value, product, profile) {
  if (value?.action !== 'check' || value.profile !== profile || value.qualified !== true ||
      value.catalog_status !== 'ready' || value.network_used !== false ||
      !DIGEST.test(value.catalog_identity ?? '') || !DIGEST.test(value.generation_identity ?? '') ||
      !Number.isSafeInteger(value.compressed_bytes) || value.compressed_bytes < 1 ||
      value.compressed_bytes > product.maximumBytes) {
    throw new Error(`Cloud accelerator catalog is not qualified: ${product.command}/${profile}`);
  }
  return value;
}

function assertInstalled(value, action, product, profile, catalog) {
  if (value?.action !== action || value.profile !== profile || value.qualified !== true ||
      value.installed !== true || value.validation !== 'ready' ||
      value.catalog_identity !== catalog.catalog_identity ||
      value.installed_generation_identity !== catalog.generation_identity ||
      value.installed_payload_bytes !== catalog.compressed_bytes) {
    throw new Error(`Cloud accelerator image is not bound to the signed catalog: ${product.command}/${profile}`);
  }
}

export function prepareComputeAccelerators({ mode, version, executable, root, invoke = invokeClearra }) {
  if (!['provision', 'verify'].includes(mode) || !isAbsolute(executable ?? '') || !isAbsolute(root ?? '')) {
    throw new Error('Cloud accelerator image requires a mode and absolute CLI/data paths');
  }
  if (!requiresV081Accelerators(version)) return { required: false, profiles: 0 };

  // Preflight all ten signed slots before the first network request. A
  // partially qualified profile set must never become an accepted image.
  const catalogs = [];
  for (const product of PRODUCTS) {
    let total = 0;
    for (const profile of PROFILES) {
      const directory = join(root, product.directory);
      const catalog = assertCatalog(request(executable, product, 'check', profile, directory, invoke), product, profile);
      total += catalog.compressed_bytes;
      catalogs.push({ product, profile, directory, catalog });
    }
    if (total > product.aggregateBytes) throw new Error(`Cloud accelerator ${product.command} aggregate exceeds its limit`);
  }

  if (mode === 'provision') {
    for (const { product, profile, directory, catalog } of catalogs) {
      mkdirSync(directory, { recursive: true });
      assertInstalled(request(executable, product, 'download', profile, directory, invoke),
        'download', product, profile, catalog);
    }
  }
  for (const { product, profile, directory, catalog } of catalogs) {
    assertInstalled(request(executable, product, 'status', profile, directory, invoke),
      'status', product, profile, catalog);
  }
  return { required: true, profiles: PROFILES.length };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    const [mode, version, executable, root] = process.argv.slice(2);
    const result = prepareComputeAccelerators({ mode, version, executable, root });
    console.log(`cloud_accelerators=${result.required ? 'qualified_immutable' : 'pre_v081'} profiles=${result.profiles}`);
  } catch (error) {
    console.error(`cloud_accelerators=failed reason=${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}
