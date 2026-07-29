const ARTIFACT_SCHEMA_VERSION = 12;
const ARTIFACT_PATH = 'tablebase/pc4-compact-exact-v12.bin';
const ARTIFACT_BYTE_LENGTH = 2_044_693;
const ARTIFACT_SHA256 = '6f4b505f6e4e322e5766273b2ec3caf0769fb152c2eb079d0a76bd44c0243fd9';

export type Pc4TablebaseAssetBundle = {
  schemaVersion: typeof ARTIFACT_SCHEMA_VERSION;
  artifactSha256: typeof ARTIFACT_SHA256;
  byteLength: number;
  artifact: ArrayBuffer;
};

let cachedBundle: Pc4TablebaseAssetBundle | null = null;
let activeDownload: Promise<Pc4TablebaseAssetBundle> | null = null;
let activeController: AbortController | null = null;
let downloadGeneration = 0;

export function prewarmPc4TablebaseAssets(): Promise<Pc4TablebaseAssetBundle> {
  if (cachedBundle) return Promise.resolve(cachedBundle);
  if (activeDownload) return activeDownload;

  const generation = ++downloadGeneration;
  const controller = new AbortController();
  activeController = controller;
  const download = downloadArtifact(controller.signal).then((bundle) => {
    if (generation !== downloadGeneration) throw abortError();
    cachedBundle = bundle;
    return bundle;
  });
  activeDownload = download.finally(() => {
    if (generation === downloadGeneration) {
      activeDownload = null;
      activeController = null;
    }
  });
  return activeDownload;
}

export function releasePc4TablebaseAssets() {
  downloadGeneration += 1;
  activeController?.abort();
  activeController = null;
  activeDownload = null;
  cachedBundle = null;
}

export function pc4TablebaseArtifactSha256(): string {
  return ARTIFACT_SHA256;
}

async function downloadArtifact(signal: AbortSignal): Promise<Pc4TablebaseAssetBundle> {
  const deploymentBase = deploymentBaseFromWorkerLocation(self.location.pathname);
  const artifactUrl = new URL(`${deploymentBase}/${ARTIFACT_PATH}`, self.location.origin);
  artifactUrl.searchParams.set('v', ARTIFACT_SHA256);
  const response = await fetch(artifactUrl, {
    cache: 'force-cache',
    credentials: 'same-origin',
    redirect: 'follow',
    signal
  });
  if (!response.ok) {
    throw new Error(`tablebase artifact returned HTTP ${response.status}`);
  }
  const artifact = await response.arrayBuffer();
  if (artifact.byteLength !== ARTIFACT_BYTE_LENGTH) {
    throw new Error(
      `tablebase artifact has ${artifact.byteLength} bytes; expected ${ARTIFACT_BYTE_LENGTH}`
    );
  }
  if ((await sha256Hex(artifact)) !== ARTIFACT_SHA256) {
    throw new Error('tablebase artifact failed SHA-256 verification');
  }
  return {
    schemaVersion: ARTIFACT_SCHEMA_VERSION,
    artifactSha256: ARTIFACT_SHA256,
    byteLength: artifact.byteLength,
    artifact
  };
}

function deploymentBaseFromWorkerLocation(pathname: string): string {
  const appMarker = '/_app/';
  const appIndex = pathname.lastIndexOf(appMarker);
  return appIndex < 0 ? '' : pathname.slice(0, appIndex);
}

async function sha256Hex(buffer: ArrayBuffer): Promise<string> {
  if (!globalThis.crypto?.subtle) {
    throw new Error('Web Crypto is unavailable; tablebase integrity cannot be verified');
  }
  const digest = await globalThis.crypto.subtle.digest('SHA-256', buffer);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

function abortError(): DOMException {
  return new DOMException('tablebase download was cancelled', 'AbortError');
}
