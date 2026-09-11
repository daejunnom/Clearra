export const UI_LANGUAGE_MANIFEST = {
  en: { status: 'released', nativeLabel: 'English', shortLabel: 'EN' },
  ko: { status: 'released', nativeLabel: '한국어', shortLabel: 'KO' },
  ja: { status: 'released', nativeLabel: '日本語', shortLabel: 'JA' }
} as const;

export type WorkspaceLocale = keyof typeof UI_LANGUAGE_MANIFEST;
export type WorkspaceLanguage = {
  [Locale in WorkspaceLocale]: (typeof UI_LANGUAGE_MANIFEST)[Locale]['status'] extends 'released'
    ? Locale
    : never;
}[WorkspaceLocale];

export const RELEASED_WORKSPACE_LANGUAGES = Object.freeze(
  (Object.keys(UI_LANGUAGE_MANIFEST) as WorkspaceLocale[]).filter(
    (locale): locale is WorkspaceLanguage => UI_LANGUAGE_MANIFEST[locale].status === 'released'
  )
);

export function matchKnownWorkspaceLocale(value?: string | null): WorkspaceLocale | null {
  if (typeof value !== 'string') return null;
  const primary = value.trim().toLowerCase().replaceAll('_', '-').split(/[-.]/u, 1)[0];
  return Object.hasOwn(UI_LANGUAGE_MANIFEST, primary) ? primary as WorkspaceLocale : null;
}

export function matchReleasedWorkspaceLanguage(value?: string | null): WorkspaceLanguage | null {
  const locale = matchKnownWorkspaceLocale(value);
  return locale !== null && UI_LANGUAGE_MANIFEST[locale].status === 'released'
    ? locale as WorkspaceLanguage
    : null;
}

export function isReleasedWorkspaceLanguage(value: unknown): value is WorkspaceLanguage {
  return typeof value === 'string' && matchReleasedWorkspaceLanguage(value) === value;
}
