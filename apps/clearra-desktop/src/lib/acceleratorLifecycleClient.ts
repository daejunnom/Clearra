// A status inspection consists of two explicit lifecycle operations. Serialize
// them to match the native owner lease rather than racing check against status.
type InspectSelection = { product: string; profile: string };
type InspectInvoke = (
  command: 'accelerator_asset_action',
  args: InspectSelection & { action: 'check' | 'status' }
) => Promise<string>;

export async function inspectAcceleratorAsset(
  invoke: InspectInvoke,
  selection: Readonly<InspectSelection>,
  isActive: () => boolean = () => true
): Promise<{ catalog: string; local: string }> {
  const { product, profile } = selection;
  const catalog = await invoke('accelerator_asset_action', { product, action: 'check', profile });
  if (!isActive()) throw new Error('accelerator_inspection_cancelled');
  const local = await invoke('accelerator_asset_action', { product, action: 'status', profile });
  return { catalog, local };
}
