// SRP: resolves one moving Hugging Face dataset reference and inventories only
// graph/index candidates. It does not map filenames to rule profiles, qualify
// completeness, sign a generation, download artifact bodies, or promote it.

export const PC4_UPSTREAM_DISCOVERY_SCHEMA =
  "clearra.pc4.upstream-discovery.v1";
export const DEFAULT_PC4_DATASET_REPOSITORY =
  "muse918/tetris-4lpc-mdp-vstar-policy";

const API_ORIGIN = "https://huggingface.co";
const GIT_OBJECT_ID = /^[0-9a-f]{40}$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const MOVING_REF = /^[A-Za-z0-9][A-Za-z0-9._/-]{0,127}$/u;
const CANDIDATE_PATH = /^(?:field_hash_to_id\.v1\.bin|graph_offsets\.u32\.bin|graph(?:_[A-Za-z0-9]+)?\.bin)$/u;
const MAX_RESPONSE_BYTES = 4 * 1024 * 1024;
const MAX_TREE_ENTRIES = 512;

export async function discoverPc4UpstreamGeneration(options = {}, dependencies = {}) {
  const repository = requirePattern(
    options.repository ?? DEFAULT_PC4_DATASET_REPOSITORY,
    REPOSITORY,
    "PC4 dataset repository",
  );
  const channel = requirePattern(options.channel ?? "main", MOVING_REF, "PC4 dataset channel");
  if (channel.includes("..") || channel.includes("//")) {
    throw new Error("PC4 dataset channel is not canonical");
  }
  const requestJson = dependencies.requestJson ?? boundedJsonRequest;
  const metadataUrl = `${API_ORIGIN}/api/datasets/${repository}/revision/${encodeURIComponent(channel)}`;
  const metadata = await requestJson(metadataUrl, "PC4 dataset revision metadata");
  const resolvedRevision = requirePattern(
    metadata?.sha,
    GIT_OBJECT_ID,
    "PC4 resolved dataset revision",
  );
  if (metadata.id !== repository || metadata.private === true || metadata.gated === true) {
    throw new Error("PC4 dataset revision metadata is not the requested public repository");
  }

  const treeUrl = `${API_ORIGIN}/api/datasets/${repository}/tree/${resolvedRevision}?recursive=false&expand=false`;
  const tree = await requestJson(treeUrl, "PC4 immutable dataset tree");
  if (!Array.isArray(tree) || tree.length === 0 || tree.length > MAX_TREE_ENTRIES) {
    throw new Error("PC4 immutable dataset tree is empty or exceeds the discovery bound");
  }

  const candidates = tree
    .filter((entry) => entry?.type === "file" && CANDIDATE_PATH.test(entry.path ?? ""))
    .map(validateCandidate)
    .sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0);
  if (!candidates.some(({ role }) => role === "field-hash-index") ||
      !candidates.some(({ role }) => role === "graph-offsets") ||
      !candidates.some(({ role }) => role === "graph-candidate")) {
    throw new Error("PC4 upstream discovery did not find the required graph/index candidates");
  }

  return deepFreeze({
    schema: PC4_UPSTREAM_DISCOVERY_SCHEMA,
    repository,
    channel,
    resolved_revision: resolvedRevision,
    candidates,
    qualification_status: "unqualified",
  });
}

async function boundedJsonRequest(url, label) {
  const response = await fetch(url, {
    method: "GET",
    headers: Object.freeze({
      accept: "application/json",
      "user-agent": "Clearra-PC4-generation-discovery/1",
    }),
    redirect: "error",
  });
  if (!response.ok || response.status !== 200) {
    throw new Error(`${label} returned HTTP ${response.status}`);
  }
  const declaredLength = response.headers.get("content-length");
  if (declaredLength !== null &&
      (!/^\d+$/u.test(declaredLength) || Number(declaredLength) > MAX_RESPONSE_BYTES)) {
    throw new Error(`${label} exceeds the bounded response size`);
  }
  if (response.body === null) throw new Error(`${label} has no response body`);
  const reader = response.body.getReader();
  const chunks = [];
  let byteLength = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    byteLength += value.byteLength;
    if (byteLength > MAX_RESPONSE_BYTES) {
      await reader.cancel("bounded PC4 discovery response exceeded");
      throw new Error(`${label} exceeds the bounded response size`);
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(byteLength);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw new Error(`${label} is not canonical UTF-8 JSON`);
  }
}

function validateCandidate(entry) {
  const path = requirePattern(entry.path, CANDIDATE_PATH, "PC4 candidate path");
  if (!Number.isSafeInteger(entry.size) || entry.size <= 0) {
    throw new Error(`PC4 candidate has an invalid byte length: ${path}`);
  }
  const digest = requirePattern(entry.lfs?.oid, SHA256, `PC4 ${path} LFS content identity`);
  if (entry.lfs.size !== entry.size) {
    throw new Error(`PC4 ${path} LFS and tree lengths differ`);
  }
  const role = path === "field_hash_to_id.v1.bin"
    ? "field-hash-index"
    : path === "graph_offsets.u32.bin"
      ? "graph-offsets"
      : "graph-candidate";
  return {
    role,
    path,
    byte_length: entry.size,
    content_identity: `sha256:${digest}`,
  };
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function deepFreeze(value) {
  Object.freeze(value);
  for (const child of Object.values(value)) {
    if (child !== null && typeof child === "object" && !Object.isFrozen(child)) {
      deepFreeze(child);
    }
  }
  return value;
}
