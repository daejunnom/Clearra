export {
  Ctk3CodecError,
  CTK3_BUNDLE_PREFIX,
  CTK3_MAX_BUNDLE_PAGES,
  CTK3_MAX_SEGMENT_PAGES,
  CTK3_PREFIX,
  decodeCtk3,
  decodeCtk3Segment,
  defaultCtk3Flags,
  encodeCtk3,
  encodeCtk3Bundle,
  encodeCtk3Compact,
  inspectCtk3,
  isCtk3,
  splitCtk3Segments,
  type Ctk3Color,
  type Ctk3Document,
  type Ctk3DocumentInfo,
  type Ctk3Operation,
  type Ctk3Page,
  type Ctk3PageFlags,
  type Ctk3Piece,
  type Ctk3Rotation,
} from "./codec.js";
export {
  Ctk3DocumentReader,
  openCtk3Document,
  type Ctk3DocumentReaderOptions,
} from "./documentReader.js";
export {
  Ctk3AsyncDocumentReader,
  decodeCtk3Async,
  encodeCtk3Async,
  encodeCtk3PageSourceAsync,
  openCtk3DocumentAsync,
  type Ctk3AsyncDecoderOptions,
  type Ctk3AsyncEncoderOptions,
  type Ctk3AsyncPageSource,
  type Ctk3DecodeWorkerFactory,
  type Ctk3DecodeWorkerLike,
  type Ctk3DecodeWorkerRequest,
  type Ctk3DecodeWorkerResponse,
} from "./asyncCodec.js";
export {
  canonicalizeCtkOperation,
  ctkOperationRotations,
  operationCells,
  operationOffsets,
  type CtkCellCoordinate,
} from "./operationGeometry.js";
export {
  Ctk3FumenCompatibilityError,
  decodeFumenCompatible,
  decoder,
  encodeFumenCompatible,
  encoder,
  type Operation,
  type Page,
  type PageRefs,
  type Pages,
} from "./fumenCompatibility.js";
export {
  CTK3_FILE_EXTENSION,
  CTK3_FILE_MIME_TYPE,
  createCtk3Blob,
  ctk3FileSource,
  decodeCtk3File,
  encodeCtk3File,
  isCtk3File,
  parseCtk3File,
  readCtk3File,
  readCtk3FileSource,
  type Ctk3FileData,
  type Ctk3FileLike,
  type ParsedCtk3File,
} from "./file.js";
export { Field, Mino, type EncodePage, type EncodePages } from "tetris-fumen";

export const documentDecoder = {
  decode: decodeDocument,
  decodeAsync: decodeDocumentAsync,
  open: openDocument,
  openAsync: openDocumentAsync,
};

export const documentEncoder = {
  encode: encodeDocument,
  encodeAsync: encodeDocumentAsync,
};
import {
  decodeCtk3 as decodeDocument,
  encodeCtk3 as encodeDocument,
} from "./codec.js";
import {
  decodeCtk3Async as decodeDocumentAsync,
  encodeCtk3Async as encodeDocumentAsync,
  openCtk3DocumentAsync as openDocumentAsync,
} from "./asyncCodec.js";
import { openCtk3Document as openDocument } from "./documentReader.js";
