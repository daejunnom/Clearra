export type CtkViewerQuery = {
  document: string | null;
  viewer: boolean;
};

export function resolveCtkViewerQuery(url: URL): CtkViewerQuery {
  const named =
    url.searchParams.get('ctk') ??
    url.searchParams.get('fumen') ??
    url.searchParams.get('document');
  const raw = rawDocumentQuery(url.search.slice(1));
  const document = named || raw;
  return {
    document,
    viewer: document !== null || url.searchParams.get('viewer') === '1'
  };
}

function rawDocumentQuery(query: string): string | null {
  if (!/^(?:v11(?:0|5)@|ctk3(?:b_|_|@))/i.test(query)) return null;
  try {
    return decodeURIComponent(query);
  } catch {
    return query;
  }
}
