export type PasteTransferItem<TFile> = {
  readonly kind?: string;
  getAsFile?(): TFile | null;
};

export type PasteTransfer<TFile> = {
  readonly files?: ArrayLike<TFile> | Iterable<TFile>;
  readonly items?: ArrayLike<PasteTransferItem<TFile>> | Iterable<PasteTransferItem<TFile>>;
  getData?(format: string): string;
};

export type DocumentPastePayload<TFile> =
  | { readonly kind: 'file'; readonly file: TFile }
  | { readonly kind: 'text'; readonly source: string };

export function selectDocumentPastePayload<TFile>(
  transfer: PasteTransfer<TFile> | null,
  matchesFile: (file: TFile) => boolean,
  matchesText: (source: string) => boolean
): DocumentPastePayload<TFile> | null {
  if (!transfer) return null;

  const file = firstMatchingFile(transfer.items, matchesFile)
    ?? firstMatchingValue(transfer.files, matchesFile);
  if (file) return { kind: 'file', file };

  const source = transfer.getData?.('text/plain')?.trim() ?? '';
  return matchesText(source) ? { kind: 'text', source } : null;
}

export function selectSingleDocumentDropFile<TFile>(
  transfer: Pick<PasteTransfer<TFile>, 'files'> | null | undefined,
  matchesFile: (file: TFile) => boolean
): TFile | null {
  const files = values(transfer?.files);
  return files.length === 1 && matchesFile(files[0]) ? files[0] : null;
}

function firstMatchingFile<TFile>(
  items: ArrayLike<PasteTransferItem<TFile>> | Iterable<PasteTransferItem<TFile>> | undefined,
  matches: (file: TFile) => boolean
): TFile | null {
  for (const item of values(items)) {
    if (item.kind !== 'file') continue;
    const file = item.getAsFile?.();
    if (file && matches(file)) return file;
  }
  return null;
}

function firstMatchingValue<T>(
  valuesLike: ArrayLike<T> | Iterable<T> | undefined,
  matches: (value: T) => boolean
): T | null {
  for (const value of values(valuesLike)) {
    if (matches(value)) return value;
  }
  return null;
}

function values<T>(source: ArrayLike<T> | Iterable<T> | undefined): T[] {
  return source ? Array.from(source) : [];
}
