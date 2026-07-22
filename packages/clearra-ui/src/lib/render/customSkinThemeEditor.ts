export type CustomSkinThemeFieldKey =
  | 'skin_id'
  | 'palette_id'
  | 'piece_mapping'
  | 'grid_style'
  | 'background'
  | 'line_clear_highlight'
  | 'ownership_color_mode'
  | 'export_limits'
  | 'provenance';

export type UserImportedSkinAssetLocation =
  | 'user_config_directory'
  | 'user_cache_directory';

const customSkinThemeFields: CustomSkinThemeFieldKey[] = [
  'skin_id',
  'palette_id',
  'piece_mapping',
  'grid_style',
  'background',
  'line_clear_highlight',
  'ownership_color_mode',
  'export_limits',
  'provenance'
];

const userImportedAssetLocations: UserImportedSkinAssetLocation[] = [
  'user_config_directory',
  'user_cache_directory'
];

export const customSkinThemeEditorContract = {
  schemaVersion: 1,
  fields: customSkinThemeFields,
  runtimePreviewSource: 'png-atlas',
  rawSvgRuntimeRendererAllowed: false,
  userImportedAssetLocations,
  repositoryAssetsAllowedForUserImports: false,
  manifestAndProvenanceRequired: true
} as const;

export function customThemePreviewUsesPngAtlas() {
  return customSkinThemeEditorContract.runtimePreviewSource === 'png-atlas';
}

export function rawSvgNotPassedToRuntimeRenderer() {
  return !customSkinThemeEditorContract.rawSvgRuntimeRendererAllowed;
}
