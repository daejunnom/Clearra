import { preferredWorkspaceLanguage, type WorkspaceLanguage } from './workspaceI18n';

let selectedLanguage: WorkspaceLanguage | null = null;
let sessionSelectionOnly = false;

/** Browser preferences must not abort workspace mounting when storage is denied. */
export function readWorkspaceLanguage(): WorkspaceLanguage {
  if (sessionSelectionOnly && selectedLanguage !== null) return selectedLanguage;
  let preference: string | null = null;
  try {
    preference = globalThis.localStorage?.getItem('clearra-language') ?? null;
  } catch {
    // Embedded/private contexts may reject access to the storage object itself.
    if (selectedLanguage !== null) return selectedLanguage;
  }
  const language = preferredWorkspaceLanguage(
    preference ?? selectedLanguage ?? globalThis.navigator?.language
  );
  selectedLanguage = language;
  return language;
}

/** Keep the active session selection even when persistence is unavailable. */
export function persistWorkspaceLanguage(language: WorkspaceLanguage): void {
  selectedLanguage = language;
  sessionSelectionOnly = true;
  try {
    const storage = globalThis.localStorage;
    if (storage) {
      storage.setItem('clearra-language', language);
      sessionSelectionOnly = false;
    }
  } catch {
    // Persistence is optional; the visible selection is already authoritative.
  }
}
