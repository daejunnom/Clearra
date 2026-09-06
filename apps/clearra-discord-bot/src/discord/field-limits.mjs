// Shared field-capacity contract for command metadata and input validation.
// Keep this leaf independent of document codecs: job telemetry also reads the
// command catalogue but must not initialize the GUI/Discord input decoder.
export const DISCORD_PC_FIELD_MAX_ROWS = 6;
export const DISCORD_WIDE_FIELD_MAX_ROWS = 24;
