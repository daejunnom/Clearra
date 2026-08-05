import type { Ctk3Color } from './ctk3Codec';

export const CTK_BOARD_WIDTH = 10 as const;

export type CtkSolidColor = Exclude<Ctk3Color, null>;

export const CTK_PALETTE_COLORS = [
  'G',
  'I',
  'O',
  'T',
  'S',
  'Z',
  'J',
  'L'
] as const satisfies readonly CtkSolidColor[];

export const CTK_COLOR_HEX = {
  G: '#7b8581',
  I: '#55cbd3',
  O: '#f3cf4d',
  T: '#b66ad0',
  S: '#65c778',
  Z: '#e96e6e',
  J: '#628ae0',
  L: '#ef9c4d'
} as const satisfies Readonly<Record<CtkSolidColor, string>>;

export const CTK_BOARD_THEME = {
  board: '#101817',
  empty: '#1e2927'
} as const;
