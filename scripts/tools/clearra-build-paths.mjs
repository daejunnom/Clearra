import { posix, resolve, win32 } from 'node:path';
import { pathToFileURL } from 'node:url';
import { assertManagedBuildTransaction, buildPathIdentity, buildSourceId, nativeBuildPath } from './clearra-build-policy.mjs';

function normalizeWindowsHostPath(value, label) {
  if (typeof value !== 'string' || !/^[A-Za-z]:[\\/][^\\/]/u.test(value) ||
      value.slice(3).split(/[\\/]/u).some(segment =>
        !segment || /[<>:"|?*\x00-\x1f]/u.test(segment) || /[ .]$/u.test(segment))) {
    throw new Error(`Invalid canonical Windows ${label} path`);
  }
  return win32.normalize(value);
}

function normalizeGuestHome(value) {
  if (typeof value !== 'string' || !value.startsWith('/') || value === '/' ||
      value === '/root' || value.startsWith('/root/') ||
      value === '/mnt' || value.startsWith('/mnt/') ||
      value !== posix.normalize(value) ||
      value.slice(1).split('/').some(segment =>
        !segment || segment === '.' || segment === '..' || /[\\\x00-\x1f\x7f]/u.test(segment))) {
    throw new Error('Invalid canonical current-user WSL home path');
  }
  return value;
}

/**
 * Prospective host-delivered bindings only. hostAnchorRoot is the supervisor's
 * verified LOCALAPPDATA (local) or RUNNER_TEMP (hosted), not a guest override
 * or a directory inferred from the requested build root. guestHome is the
 * separately established current user's ext4 home. This pure check
 * constrains the path roles, not filesystem ownership or host authentication.
 * The live path CLI still reads only the existing managed build owner.
 */
export function validateWslBuildBinding({
  hostMode, hostAnchorRoot, hostCheckoutRoot, hostBuildRoot,
  checkoutId, guestBuildRoot, guestHome, guestSourceRoot, owner,
}) {
  if (hostMode !== 'local' && hostMode !== 'github-actions') throw new Error('Unknown Windows build host mode');
  const anchor = normalizeWindowsHostPath(hostAnchorRoot, 'build anchor');
  const checkout = normalizeWindowsHostPath(hostCheckoutRoot, 'checkout');
  const providedBuild = normalizeWindowsHostPath(hostBuildRoot, 'build root');
  if (hostMode === 'github-actions' && win32.parse(anchor).root.toLowerCase() !== win32.parse(checkout).root.toLowerCase()) {
    throw new Error('Hosted WSL build anchor must share the checkout volume');
  }
  const expectedHostBuild = win32.join(anchor, 'Clearra', 'build');
  if (buildPathIdentity(providedBuild) !== buildPathIdentity(expectedHostBuild)) {
    throw new Error('WSL build root does not match the exact host cache role');
  }
  const expectedGuestBuild = nativeBuildPath(expectedHostBuild, 'linux');
  if (guestBuildRoot !== expectedGuestBuild) throw new Error('WSL build root does not match the exact Windows mount');
  if (checkoutId !== buildSourceId(checkout)) throw new Error('WSL build checkout identity mismatch');
  const expectedGuestSource = `${normalizeGuestHome(guestHome)}/.local/share/Clearra/workspaces/${checkoutId}/source`;
  if (guestSourceRoot !== expectedGuestSource) throw new Error('WSL build source does not match the exact checkout slot');

  // The stable host checkout slot is a transport identity. The existing
  // build owner still hashes the ext4 source path, preventing cross-seeding.
  const sourceId = buildSourceId(expectedGuestSource);
  const paths = { checkoutId, sourceId, root: expectedGuestBuild, sourceRoot: expectedGuestSource };
  if (owner === undefined) return paths;
  if (!owner || owner.sourceId !== sourceId || typeof owner.sessionId !== 'string' || !/^[a-f0-9]{32}$/u.test(owner.sessionId)) {
    throw new Error('WSL build owner identity mismatch');
  }
  let transaction;
  if (owner.purpose === 'experiment') {
    transaction = `${expectedGuestBuild}/experiments/${sourceId}/current`;
  } else if (owner.purpose === 'product') {
    if (typeof owner.createdUtc !== 'string' || !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/u.test(owner.createdUtc)) {
      throw new Error('WSL product generation requires its exact UTC timestamp');
    }
    const created = new Date(owner.createdUtc);
    if (!Number.isFinite(created.valueOf()) || created.toISOString() !== owner.createdUtc) {
      throw new Error('WSL product generation requires its exact UTC timestamp');
    }
    transaction = `${expectedGuestBuild}/products/${owner.createdUtc.replace(/[-:.]/gu, '')}-${owner.sessionId}`;
  } else {
    throw new Error('Unknown WSL build owner purpose');
  }
  const cargoTarget = `${transaction}/cargo-target`;
  if (owner.transactionRoot !== transaction || owner.cargoTarget !== cargoTarget) {
    throw new Error('WSL build output does not match the exact transaction role');
  }
  return { ...paths, transaction, cargoTarget };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  let field;
  let sourceRoot;
  const arguments_ = process.argv.slice(2);
  if (arguments_.length % 2 !== 0) throw new Error('Build path options require values');
  for (let index = 0; index < arguments_.length; index += 2) {
    if (arguments_[index] === '--field') field = arguments_[index + 1];
    else if (arguments_[index] === '--source-root') sourceRoot = arguments_[index + 1];
    else throw new Error('Unknown build path option');
  }
  const owner = assertManagedBuildTransaction({ sourceRoot });
  const values = { root: owner.root, transaction: owner.transaction, 'cargo-target': owner.cargoTarget, 'source-root': owner.source_root };
  if (!Object.hasOwn(values, field)) throw new Error('Unknown managed build path field');
  console.log(values[field]);
}
