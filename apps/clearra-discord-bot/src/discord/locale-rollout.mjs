export const DISCORD_LOCALE_MANIFEST = Object.freeze({
  en: Object.freeze({ status: "released", nativeLabel: "English" }),
  ko: Object.freeze({ status: "released", nativeLabel: "한국어" }),
  ja: Object.freeze({ status: "released", nativeLabel: "日本語" }),
});

export const SUPPORTED_DISCORD_LOCALES = Object.freeze(
  Object.entries(DISCORD_LOCALE_MANIFEST)
    .filter(([, definition]) => definition.status === "released")
    .map(([locale]) => locale),
);

export const PLANNED_DISCORD_LOCALES = Object.freeze(
  Object.entries(DISCORD_LOCALE_MANIFEST)
    .filter(([, definition]) => definition.status === "planned")
    .map(([locale]) => locale),
);

export function matchKnownDiscordLocale(value) {
  if (typeof value !== "string") return null;
  const [primary] = value.trim().toLowerCase().replaceAll("_", "-").split(/[-.]/u, 1);
  return Object.hasOwn(DISCORD_LOCALE_MANIFEST, primary) ? primary : null;
}

export function isReleasedDiscordLocale(value) {
  return typeof value === "string" && SUPPORTED_DISCORD_LOCALES.includes(value);
}

export function releasedDiscordLocaleList(conjunction = "or") {
  if (SUPPORTED_DISCORD_LOCALES.length < 2) return SUPPORTED_DISCORD_LOCALES.join("");
  return `${SUPPORTED_DISCORD_LOCALES.slice(0, -1).join(", ")} ${conjunction} ${SUPPORTED_DISCORD_LOCALES.at(-1)}`;
}
