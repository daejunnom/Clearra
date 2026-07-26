<script lang="ts">
  import { CircleHelp } from '@lucide/svelte';

  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let language: WorkspaceLanguage;
  export let explainInitialHold = false;
  export let mode: 'pattern' | 'setup' | 'setup-qb' = 'pattern';

  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
</script>

<details class="pattern-help">
  <summary>
    <CircleHelp size={14} strokeWidth={1.8} />
    {label(mode === 'pattern' ? 'queuePatternHelp' : 'setupQueueSyntax')}
  </summary>
  <dl>
    {#if mode === 'setup-qb'}
      <div><dt>TI + OS</dt><dd>{label('setupQbQueueLetters')}</dd></div>
      <div class="wide"><dt>[TI]![OS]!</dt><dd>{label('setupQbAllPieces')}</dd></div>
      <div class="wide"><dt>≤ 7</dt><dd>{label('setupQbSevenLimit')}</dd></div>
    {:else if mode === 'setup'}
      <div><dt>IOTS</dt><dd>{label('setupQueueLetters')}</dd></div>
      <div><dt>SIOS</dt><dd>{label('setupQueueInitialHold')}</dd></div>
      <div class="wide"><dt>7,4,1,5,2,6,3</dt><dd>{label('setupQueueCycle')}</dd></div>
      <div class="wide"><dt>P7 / [...] / !</dt><dd>{label('setupQueueNoPattern')}</dd></div>
    {:else}
      <div><dt>IOTSZJL</dt><dd>{label('queuePatternExact')}</dd></div>
      <div><dt>PN / P7P3</dt><dd>{label('queuePatternP4')}</dd></div>
      <div><dt>[OISZ] / [^TIZ]</dt><dd>{label('queuePatternChoice')}</dd></div>
      <div><dt>[...]N / [...]!</dt><dd>{label('queuePatternSuffix')}</dd></div>
      {#if explainInitialHold}
        <div class="wide"><dt>{label('holdPiece')}</dt><dd>{label('initialHoldHelp')}</dd></div>
      {/if}
      <div class="wide reference">
        <dt>{label('queuePatternReferenceLabel')}</dt>
        <dd>
          {label('queuePatternReferenceDifference')}
          <a href="https://hsterts.github.io/h-docs/sfinder/parameter-patterns/" target="_blank" rel="noreferrer">
            {label('queuePatternReferenceLink')}
          </a>
        </dd>
      </div>
    {/if}
  </dl>
</details>

<style>
  .pattern-help {
    color: #5e6a65;
    font-size: 11px;
    margin-top: 9px;
  }

  .pattern-help summary {
    align-items: center;
    cursor: pointer;
    display: inline-flex;
    gap: 5px;
    list-style: none;
    margin: 0;
  }

  .pattern-help summary::-webkit-details-marker {
    display: none;
  }

  .pattern-help dl {
    background: #f4f7f5;
    border: 1px solid #d9dfdb;
    border-radius: 5px;
    display: grid;
    gap: 6px;
    margin: 9px 0 0;
    padding: 9px 10px;
  }

  .pattern-help dl > div {
    display: grid;
    gap: 8px;
    grid-template-columns: 76px minmax(0, 1fr);
  }

  .pattern-help dl > .wide {
    grid-template-columns: 104px minmax(0, 1fr);
  }

  .pattern-help .reference a {
    color: #155f59;
    font-weight: 700;
    margin-left: 4px;
  }

  .pattern-help dt {
    color: #155f59;
    font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
    font-weight: 800;
  }

  .pattern-help dd {
    margin: 0;
  }
</style>
