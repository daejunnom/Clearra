<script lang="ts">
  import type { RenderCapabilityReport } from './renderCapabilityReport';

  export let capability: RenderCapabilityReport | null;

  $: formatsSupported = Boolean(capability?.png_supported && capability?.gif_supported);
  $: supportStatus = capability === null ? 'pending' : formatsSupported ? 'supported' : 'unsupported';
  $: exactStatus = capability === null ? 'pending' : String(capability.render_exact);
  $: reason = capability === null ? 'pending' : capability.unsupported_reason ?? 'none';
</script>

<section class="panel" aria-label="Render status">
  <h2>Render</h2>
  <dl>
    <div>
      <dt>PNG/GIF</dt>
      <dd>{supportStatus}</dd>
    </div>
    <div>
      <dt>Exact</dt>
      <dd>{exactStatus}</dd>
    </div>
    <div>
      <dt>Reason</dt>
      <dd>{reason}</dd>
    </div>
  </dl>
</section>

<style>
  .panel {
    border: 1px solid #2b2d33;
    border-radius: 8px;
    background: #17191f;
    padding: 16px;
  }

  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 700;
  }

  dl {
    display: grid;
    gap: 12px;
    margin: 16px 0 0;
  }

  div {
    display: flex;
    justify-content: space-between;
    gap: 16px;
  }

  dt {
    color: #a1a1aa;
    font-size: 13px;
  }

  dd {
    margin: 0;
    font-size: 13px;
  }
</style>
