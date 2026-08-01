export function buildClearraViewerUrl(baseUrl, document) {
  const url = buildClearraRendererUrl(baseUrl);
  url.searchParams.set(document.format === "ctk3" ? "ctk" : "fumen", document.source);
  return url.href;
}

export function buildClearraRendererUrl(baseUrl) {
  const url = new URL(baseUrl);
  url.search = "";
  url.hash = "";
  url.searchParams.set("tool", "ctk");
  url.searchParams.set("viewer", "1");
  return url;
}
