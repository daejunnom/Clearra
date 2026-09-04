import type {
  ClearraFieldDocumentPayload,
  ClearraRenderArtifactPayload
} from '../wasm/wasmCommandClient';
import type { ClearraDesktopCliCommandRequest } from '../host/clearraDesktopHost';
import {
  cliCommandRequestForDesktop,
  serializeCliCommandArguments
} from './cliCommandModel';

export const FIELD_DOCUMENT_MAX_INPUT_BYTES = 16 * 1024 * 1024;
export const FIELD_DOCUMENT_MAX_PAGES = 4096;

export type DocumentUtilityTool = 'parity' | 'fumen' | 'render' | 'to-gray' | 'mirror';
export type FieldDocumentFormat = 'ctk3' | 'fumen';
export type DocumentUtilityFumenTransform =
  | 'roundtrip'
  | 'combine'
  | 'split'
  | 'get-page'
  | 'page-shift'
  | 'clean-comments'
  | 'preserve-comments'
  | 'to-gray'
  | 'mirror'
  | 'text-to-fumen';

export type DocumentUtilityCommandInput = {
  tool: DocumentUtilityTool;
  format: FieldDocumentFormat;
  document: string;
  transform: DocumentUtilityFumenTransform;
  documents: readonly string[];
  pageNumber: number;
  pageShift: number;
  comments: readonly string[];
  artifactFormat: 'png' | 'gif';
};

export function buildDocumentUtilityCommandArguments(
  input: DocumentUtilityCommandInput
): string[] {
  if (input.tool === 'parity') {
    return [
      'clearra', 'utility', 'parity',
      '--format', input.format,
      '--document', input.document
    ];
  }
  if (input.tool === 'render') {
    const arguments_ = [
      'clearra', 'utility', 'render',
      '--format', input.format,
      '--document', input.document,
      '--artifact-format', input.artifactFormat
    ];
    if (input.artifactFormat === 'png') {
      arguments_.push('--page', String(input.pageNumber));
    }
    return arguments_;
  }
  if (input.tool === 'to-gray' || input.tool === 'mirror') {
    return [
      'clearra', 'utility', input.tool,
      '--format', input.format,
      '--document', input.document
    ];
  }
  const arguments_ = ['clearra', 'utility', 'fumen', input.transform, '--format', 'fumen'];
  if (input.transform !== 'text-to-fumen') {
    for (const document of input.documents) arguments_.push('--document', document);
  }
  if (input.transform === 'get-page') {
    arguments_.push('--page', String(input.pageNumber));
  }
  if (input.transform === 'page-shift') {
    arguments_.push('--offset', String(input.pageShift));
  }
  if (input.transform === 'text-to-fumen') {
    for (const comment of input.comments) arguments_.push('--comment', comment);
  }
  return arguments_;
}

export function buildDocumentUtilityCommand(input: DocumentUtilityCommandInput): string {
  return serializeCliCommandArguments(buildDocumentUtilityCommandArguments(input));
}

export function documentUtilityRequestForDesktop(
  input: DocumentUtilityCommandInput,
  language: 'en' | 'ko'
): ClearraDesktopCliCommandRequest {
  return cliCommandRequestForDesktop(buildDocumentUtilityCommandArguments(input), language);
}

export function detectFieldDocumentFormat(source: string): FieldDocumentFormat | null {
  const normalized = source.trim();
  if (/^ctk3(?:b_|_|@)/.test(normalized)) return 'ctk3';
  if (/^(?:v115|[Ddm]115)@/.test(normalized)) return 'fumen';
  return null;
}

export function isBoundedCanonicalFieldDocument(
  source: string,
  requiredFormat?: FieldDocumentFormat
): boolean {
  const normalized = source.trim();
  const format = detectFieldDocumentFormat(normalized);
  return (
    format !== null &&
    (requiredFormat === undefined || format === requiredFormat) &&
    !/\s/.test(normalized) &&
    new TextEncoder().encode(normalized).byteLength <= FIELD_DOCUMENT_MAX_INPUT_BYTES
  );
}

export function fumenDocumentInputs(source: string, combine: boolean): string[] {
  const documents = combine
    ? source.split(/\r?\n/u).map((value) => value.trim()).filter(Boolean)
    : [source.trim()];
  return documents.every((document) => isBoundedCanonicalFieldDocument(document, 'fumen'))
    ? documents
    : [];
}

export function validateFieldDocumentPayload(payload: ClearraFieldDocumentPayload): string | null {
  if (
    !isBoundedCanonicalFieldDocument(payload.document, payload.format) ||
    !Number.isInteger(payload.page_count) ||
    payload.page_count < 1 ||
    payload.page_count > FIELD_DOCUMENT_MAX_PAGES ||
    !/^[0-9a-f]{64}$/u.test(payload.canonical_sha256) ||
    !isSafeFilename(payload.filename)
  ) {
    return 'invalid field document payload';
  }
  return null;
}

export async function decodeValidatedRenderArtifact(
  payload: ClearraRenderArtifactPayload
): Promise<Uint8Array> {
  if (
    payload.render_exact !== true ||
    !Number.isSafeInteger(payload.byte_length) ||
    payload.byte_length < 1 ||
    payload.byte_length > payload.product_max_bytes ||
    payload.byte_length > payload.transport_max_bytes ||
    !/^[0-9a-f]{64}$/u.test(payload.sha256) ||
    !isSafeFilename(payload.filename) ||
    (payload.artifact_format === 'png' && payload.media_type !== 'image/png') ||
    (payload.artifact_format === 'gif' && payload.media_type !== 'image/gif')
  ) {
    throw new Error('invalid render artifact metadata');
  }
  const bytes = decodeBase64(payload.bytes_base64);
  if (bytes.byteLength !== payload.byte_length) {
    throw new Error('render artifact byte length does not match its typed metadata');
  }
  if (payload.artifact_format === 'png' ? !hasPngSignature(bytes) : !hasGifSignature(bytes)) {
    throw new Error('render artifact signature does not match its typed format');
  }
  const digestInput = new Uint8Array(new ArrayBuffer(bytes.byteLength));
  digestInput.set(bytes);
  const digest = await crypto.subtle.digest('SHA-256', digestInput);
  const sha256 = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
  if (sha256 !== payload.sha256) {
    throw new Error('render artifact digest does not match its typed metadata');
  }
  return bytes;
}

function decodeBase64(value: string): Uint8Array {
  if (!value || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/u.test(value)) {
    throw new Error('render artifact is not canonical base64');
  }
  let decoded: string;
  try {
    decoded = atob(value);
  } catch {
    throw new Error('render artifact is not decodable base64');
  }
  return Uint8Array.from(decoded, (character) => character.charCodeAt(0));
}

function hasPngSignature(bytes: Uint8Array): boolean {
  const signature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  return bytes.length >= signature.length && signature.every((byte, index) => bytes[index] === byte);
}

function hasGifSignature(bytes: Uint8Array): boolean {
  if (bytes.length < 6) return false;
  const header = String.fromCharCode(...bytes.subarray(0, 6));
  return header === 'GIF87a' || header === 'GIF89a';
}

function isSafeFilename(value: string): boolean {
  return /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/u.test(value) && !value.includes('..');
}
