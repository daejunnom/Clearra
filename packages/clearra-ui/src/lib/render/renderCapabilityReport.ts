export type RenderCapabilityReport = {
  png_supported: boolean;
  gif_supported: boolean;
  render_exact: boolean;
  unsupported_reason: string | null;
};
