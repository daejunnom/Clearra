<script lang="ts">
  import { runtimeShellCopy, runtimeShellValue } from '../i18n/runtimeShellCatalog';
  import { matchReleasedWorkspaceLanguage, type WorkspaceLanguage } from '../i18n/languageManifest';

  import type { RenderCapabilityReport } from './renderCapabilityReport';

  export let capability: RenderCapabilityReport | null;
  export let language: WorkspaceLanguage = 'en';

  $: locale = matchReleasedWorkspaceLanguage(language) ?? 'en';
  $: copy = runtimeShellCopy(locale);

  $: formatsSupported = Boolean(capability?.png_supported && capability?.gif_supported);
  $: supportStatus = capability === null ? copy.pending : formatsSupported ? copy.supported : copy.unsupported;
  $: exactStatus = capability === null ? copy.pending : runtimeShellValue(locale, capability.render_exact);
  $: reason = capability === null ? copy.pending : runtimeShellValue(locale, capability.unsupported_reason ?? 'none');
</script>

<section class="panel" aria-label={copy.renderStatus}>
  <h2>{copy.render}</h2>
  <dl>
    <div>
      <dt>PNG/GIF</dt>
      <dd>{supportStatus}</dd>
    </div>
    <div>
      <dt>{copy.exact}</dt>
      <dd>{exactStatus}</dd>
    </div>
    <div>
      <dt>{copy.reason}</dt>
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
