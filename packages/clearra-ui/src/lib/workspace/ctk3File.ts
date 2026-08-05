import {
  CTK3_FILE_EXTENSION,
  CTK3_FILE_MIME_TYPE,
  isCtk3File,
  readCtk3FileSource,
} from './ctk3Codec';
import {
  selectDocumentPastePayload,
  selectSingleDocumentDropFile
} from './documentPaste';

export const CTK3_FILE_ACCEPT = `${CTK3_FILE_EXTENSION},${CTK3_FILE_MIME_TYPE}`;

export type DocumentPasteCallbacks = {
  importSource(source: string): void | Promise<void>;
  importFailed?(): void;
};

export type DocumentDropCallbacks = DocumentPasteCallbacks & {
  dragActive?(active: boolean): void;
};

export function installGlobalDocumentPaste(
  callbacks: DocumentPasteCallbacks
): () => void {
  let active = true;
  const handlePaste = (event: ClipboardEvent) => {
    const payload = selectDocumentPastePayload(
      event.clipboardData,
      isCtk3File,
      looksLikeDocument
    );
    if (payload?.kind === 'file') {
      event.preventDefault();
      void readCtk3FileSource(payload.file)
        .then((source) => {
          if (!active) return;
          return callbacks.importSource(source);
        })
        .catch(() => {
          if (active) callbacks.importFailed?.();
        });
      return;
    }
    if (isEditableTarget(event.target)) return;
    if (payload?.kind !== 'text') return;
    event.preventDefault();
    void Promise.resolve(callbacks.importSource(payload.source)).catch(() => {
      if (active) callbacks.importFailed?.();
    });
  };
  window.addEventListener('paste', handlePaste);
  return () => {
    active = false;
    window.removeEventListener('paste', handlePaste);
  };
}

export function installGlobalDocumentDrop(
  callbacks: DocumentDropCallbacks
): () => void {
  let active = true;
  let dragDepth = 0;
  const setDragActive = (next: boolean) => {
    if (active) callbacks.dragActive?.(next);
  };
  const handleDragEnter = (event: DragEvent) => {
    if (!hasDraggedFiles(event.dataTransfer)) return;
    event.preventDefault();
    dragDepth += 1;
    setDragActive(true);
  };
  const handleDragOver = (event: DragEvent) => {
    if (!hasDraggedFiles(event.dataTransfer)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
    setDragActive(true);
  };
  const handleDragLeave = (event: DragEvent) => {
    if (!hasDraggedFiles(event.dataTransfer)) return;
    dragDepth = Math.max(0, dragDepth - 1);
    if (dragDepth === 0) setDragActive(false);
  };
  const handleDrop = (event: DragEvent) => {
    if (!hasDraggedFiles(event.dataTransfer)) return;
    event.preventDefault();
    dragDepth = 0;
    setDragActive(false);
    const file = selectCtk3DropFile(event.dataTransfer);
    if (!file) {
      callbacks.importFailed?.();
      return;
    }
    void sourceFromCtk3File(file)
      .then((source) => {
        if (!active) return;
        return callbacks.importSource(source);
      })
      .catch(() => {
        if (active) callbacks.importFailed?.();
      });
  };
  window.addEventListener('dragenter', handleDragEnter);
  window.addEventListener('dragover', handleDragOver);
  window.addEventListener('dragleave', handleDragLeave);
  window.addEventListener('drop', handleDrop);
  return () => {
    active = false;
    callbacks.dragActive?.(false);
    window.removeEventListener('dragenter', handleDragEnter);
    window.removeEventListener('dragover', handleDragOver);
    window.removeEventListener('dragleave', handleDragLeave);
    window.removeEventListener('drop', handleDrop);
  };
}

export function selectCtk3DropFile(
  dataTransfer: Pick<DataTransfer, 'files'> | null | undefined
): File | null {
  return selectSingleDocumentDropFile(dataTransfer, isCtk3File);
}

export async function sourceFromCtk3File(file: File): Promise<string> {
  if (!isCtk3File(file)) throw new Error('The selected file is not a CTK3 document.');
  return readCtk3FileSource(file);
}

export function saveCtk3Source(source: string, fileName = 'clearra.ctk3'): void {
  const normalized = source.trim();
  if (!normalized) throw new Error('The CTK3 document is empty.');
  const blob = new Blob([normalized], { type: CTK3_FILE_MIME_TYPE });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = normalizedFileName(fileName);
  anchor.style.display = 'none';
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}

function looksLikeDocument(source: string): boolean {
  return /(?:^|\s)(?:ctk3(?:b_|_|@)|v11(?:0|5)@)/i.test(source);
}

function hasDraggedFiles(dataTransfer: DataTransfer | null): boolean {
  return Array.from(dataTransfer?.types ?? []).includes('Files');
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}

function normalizedFileName(fileName: string): string {
  const safe = fileName.trim().replace(/[<>:"/\\|?*\u0000-\u001f]/g, '_') || 'clearra';
  return safe.toLowerCase().endsWith(CTK3_FILE_EXTENSION)
    ? safe
    : `${safe}${CTK3_FILE_EXTENSION}`;
}
