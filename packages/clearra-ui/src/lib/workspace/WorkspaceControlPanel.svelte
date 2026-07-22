<script lang="ts">
  export let ariaLabel: string;
</script>

<aside class="control-panel" aria-label={ariaLabel}>
  <slot />
</aside>

<style>
  .control-panel { min-width: 0; }
  .control-panel :global(.workspace-control-section) { border-bottom: 1px solid #dfe4e1; padding: 0 0 20px; }
  .control-panel :global(.workspace-control-section + .workspace-control-section) { padding-top: 20px; }
  .control-panel :global(.workspace-control-heading) { align-items: center; color: #26322e; display: flex; font-size: 13px; font-weight: 750; gap: 7px; margin: 0 0 12px; }
  .control-panel :global(.workspace-field), .control-panel :global(.workspace-field-grid), .control-panel :global(.workspace-toggle-grid) { display: grid; gap: 8px; }
  .control-panel :global(.workspace-field-grid) { gap: 12px; grid-template-columns: repeat(2, minmax(0, 1fr)); margin-top: 12px; }
  .control-panel :global(.workspace-toggle-grid) { gap: 10px 16px; grid-template-columns: repeat(2, minmax(0, 1fr)); margin-top: 14px; }
  .control-panel :global(.workspace-field.wide) { margin-top: 12px; }
  .control-panel :global(.workspace-field > span), .control-panel :global(.workspace-field-label) { color: #68736f; font-size: 11px; font-weight: 650; }
  .control-panel :global(input), .control-panel :global(select) { background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; color: #17211e; font: inherit; font-size: 13px; height: 38px; min-width: 0; padding: 0 10px; width: 100%; }
  .control-panel :global(input:focus), .control-panel :global(select:focus) { border-color: #16877d; box-shadow: 0 0 0 3px #16877d1f; outline: none; }
  .control-panel :global(input:disabled), .control-panel :global(select:disabled) { background: #edf0ee; color: #8b9591; }
  .control-panel :global(.workspace-queue-input) { color: #1d544f; font-family: ui-monospace, SFMono-Regular, Consolas, monospace; font-size: 14px; font-weight: 750; }
  .control-panel :global(.workspace-field-help) { color: #7a8580; font-size: 10px; line-height: 1.45; }
  .control-panel :global(.workspace-switch-row) { align-items: end; display: flex; gap: 18px; justify-content: space-between; margin-top: 15px; }
  .control-panel :global(.workspace-switch-label) { align-items: center; color: #34403c; cursor: pointer; display: inline-flex; font-size: 12px; font-weight: 650; gap: 8px; min-width: 0; }
  .control-panel :global(.workspace-switch-label > input) { height: 1px; opacity: 0; position: absolute; width: 1px; }
  .control-panel :global(.workspace-switch) { background: #aeb8b4; border-radius: 10px; display: block; flex: 0 0 auto; height: 18px; position: relative; transition: background 120ms ease; width: 32px; }
  .control-panel :global(.workspace-switch::after) { background: #fff; border-radius: 50%; box-shadow: 0 1px 2px #00000026; content: ''; height: 14px; left: 2px; position: absolute; top: 2px; transition: transform 120ms ease; width: 14px; }
  .control-panel :global(.workspace-switch-label input:checked + .workspace-switch) { background: #16877d; }
  .control-panel :global(.workspace-switch-label input:checked + .workspace-switch::after) { transform: translateX(14px); }
  .control-panel :global(.workspace-switch-label input:focus-visible + .workspace-switch) { outline: 2px solid #16877d; outline-offset: 2px; }
  .control-panel :global(.workspace-switch-label input:disabled + .workspace-switch), .control-panel :global(.workspace-switch-label input:disabled ~ span) { cursor: default; opacity: .45; }
  .control-panel :global(.workspace-inline-select) { align-items: center; display: flex; gap: 8px; }
  .control-panel :global(.workspace-inline-select select) { width: 122px; }
  .control-panel :global(.workspace-contract-band) { align-items: center; background: #eef2ef; color: #5e6a65; display: grid; font-size: 11px; gap: 8px; grid-template-columns: auto 1fr auto; margin: 14px 0 0; padding: 10px; }
  .control-panel :global(.workspace-contract-band b) { color: #173f3a; }
  .control-panel :global(.workspace-validation) { background: #fff2ed; border: 1px solid #ecc8ba; border-radius: 5px; color: #8d3d24; display: grid; font-size: 11px; gap: 5px; list-style: none; margin: 16px 0 0; padding: 10px 12px; }
  .control-panel :global(.workspace-validation p), .control-panel :global(.workspace-validation li) { margin: 0; }
  .control-panel :global(.workspace-segmented) { background: #e9eeeb; border-radius: 6px; display: grid; gap: 2px; padding: 3px; }
  .control-panel :global(.workspace-segmented.four) { grid-template-columns: repeat(4, minmax(0, 1fr)); }
  .control-panel :global(.workspace-segmented.two) { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .control-panel :global(.workspace-segmented button) { background: transparent; border: 0; border-radius: 4px; color: #55605c; cursor: pointer; font-size: 12px; font-weight: 700; min-height: 34px; min-width: 0; padding: 0 8px; }
  .control-panel :global(.workspace-segmented button.active) { background: #fff; box-shadow: 0 1px 3px #17211e1c; color: #075f58; }

  @media (max-width: 560px) {
    .control-panel :global(.workspace-field-grid), .control-panel :global(.workspace-toggle-grid) { grid-template-columns: 1fr; }
    .control-panel :global(.workspace-segmented.four) { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .control-panel :global(.workspace-switch-row) { align-items: stretch; flex-direction: column; }
  }
</style>
