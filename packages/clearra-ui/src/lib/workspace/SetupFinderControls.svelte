<script lang="ts">
  import { Database, Layers3, ShieldCheck } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import QueueTextInput from '../components/QueueTextInput.svelte';
  import QueuePatternHelp from './QueuePatternHelp.svelte';
  import {
    nextSetupCycleRemainingCount,
    setupCycle,
    type SetupCandidatePriority,
    type SetupFinderRequest,
    type SetupFinderValidationCode,
    type SetupLengthPreference,
    type SetupSearchMode
  } from './setupFinderModel';
  import type { RuleProfile } from './solverWorkspaceModel';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let request: SetupFinderRequest;
  export let language: WorkspaceLanguage;
  export let validationCodes: SetupFinderValidationCode[] = [];

  const dispatch = createEventDispatcher<{ change: SetupFinderRequest }>();
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);
  $: cycle = setupCycle(request.remaining);
  $: nextRemainingCount = nextSetupCycleRemainingCount(request.remaining);

  function update(change: Partial<SetupFinderRequest>) {
    dispatch('change', { ...request, ...change });
  }
</script>

<WorkspaceControlPanel ariaLabel={label('setupFinder')}>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading">
      <Database size={16} strokeWidth={1.8} />{label('setupResidue')}
    </h2>
    <label class="workspace-field wide">
      <span>{label('setupSearchMode')}</span>
      <select
        value={request.searchMode}
        on:change={(event) => {
          const searchMode = (event.currentTarget as HTMLSelectElement).value as SetupSearchMode;
          update({
            searchMode
          });
        }}
      >
        <option value="oracle">{label('setupModeOracle')}</option>
        <option value="qb">{label('setupModeQb')}</option>
      </select>
      <small class="workspace-field-help">
        {label(request.searchMode === 'qb' ? 'setupModeQbHelp' : 'setupModeOracleHelp')}
      </small>
    </label>
    <label class="workspace-field wide">
      <span>{label('remainingPieces')}</span>
      <QueueTextInput
        class="workspace-queue-input"
        value={request.remaining}
        maxlength="16"
        placeholder="IOTSZJL"
        spellcheck="false"
        aria-invalid={validationCodes.length > 0}
        on:value={(event) => update({ remaining: event.detail })}
      />
      <small class="workspace-field-help">{label('setupResidueHelp')}</small>
    </label>
    {#if request.searchMode === 'qb'}
      <label class="workspace-field wide">
        <span>{label('setupObservedQueue')}</span>
        <QueueTextInput
          class="workspace-queue-input"
          value={request.qbQueue}
          maxlength="7"
          placeholder="OS"
          spellcheck="false"
          aria-invalid={validationCodes.length > 0}
          on:value={(event) => update({ qbQueue: event.detail })}
        />
        <small class="workspace-field-help">{label('setupQbInputHelp')}</small>
      </label>
    {/if}
    <label class="workspace-field wide">
      <span>{label('setupNextCycleRemaining')}</span>
      <QueueTextInput
        class="workspace-queue-input"
        value={request.nextCycleRemaining}
        maxlength="7"
        placeholder={label('setupNextCycleOptionalPlaceholder')}
        spellcheck="false"
        aria-invalid={validationCodes.length > 0}
        on:value={(event) => update({ nextCycleRemaining: event.detail })}
      />
      <small class="workspace-field-help">{label('setupNextCycleRemainingHelp')}</small>
    </label>
    <QueuePatternHelp {language} mode={request.searchMode === 'qb' ? 'setup-qb' : 'setup'} />
    <label class="workspace-field wide queue-knowledge-field">
      <span>{label('queueKnowledge')}</span>
      <select
        value={request.queueKnowledge}
        on:change={(event) => update({
          queueKnowledge: (event.currentTarget as HTMLSelectElement).value as SetupFinderRequest['queueKnowledge']
        })}
      >
        <option value="oracle">{label('queueKnowledgeOracle')}</option>
        <option value="visible-7">{label('queueKnowledgeVisibleSeven')}</option>
      </select>
      <small class="workspace-field-help">
        {label(request.queueKnowledge === 'visible-7'
          ? 'queueKnowledgeVisibleSevenHelp'
          : 'queueKnowledgeOracleHelp')}
      </small>
    </label>

    <div class="residue-facts">
      <span>{label('pcCycle')}</span><strong>{cycle ? label('cycleNumber', { cycle }) : '—'}</strong>
      {#if request.searchMode === 'qb'}
        <span>{label('setupObservedPieces')}</span>
        <strong>{request.qbQueue.replace(/[\s,]/g, '').length}</strong>
      {/if}
      <span>{label('setupNextCycleRemainingCount')}</span>
      <strong>
        {request.nextCycleRemaining
          ? request.nextCycleRemaining.replace(/[\s,]/g, '').length
          : '—'}/{nextRemainingCount ?? '—'}
      </strong>
    </div>
  </section>

  <section class="workspace-control-section">
    <h2 class="workspace-control-heading">
      <Layers3 size={16} strokeWidth={1.8} />{label('setupSearchContract')}
    </h2>
    <div class="workspace-contract-band">
      <ShieldCheck size={15} strokeWidth={1.8} />
      <span>{label('pcTarget')}</span>
      <b>10×4</b>
    </div>
    <label class="workspace-field wide priority-field">
      <span>{label('rule')}</span>
      <select
        value={request.rule}
        on:change={(event) => update({
          rule: (event.currentTarget as HTMLSelectElement).value as RuleProfile
        })}
      >
        <option value="srs-plus">SRS+</option>
        <option value="srs">SRS</option>
        <option value="srs-x">SRS-X</option>
        <option value="jstris-180">Jstris 180</option>
      </select>
    </label>
    <label class="workspace-field wide priority-field">
      <span>{label('setupCandidatePriority')}</span>
      <select
        value={request.candidatePriority}
        on:change={(event) => update({
          candidatePriority: (event.currentTarget as HTMLSelectElement).value as SetupCandidatePriority
        })}
      >
        <option value="all">{label('setupPriorityAll')}</option>
        <option value="build">{label('setupPriorityBuild')}</option>
        <option value="pc">{label('setupPriorityPc')}</option>
      </select>
      <small class="workspace-field-help">{label('setupPriorityHelp')}</small>
    </label>
    <label class="workspace-field wide priority-field">
      <span>{label('setupLengthPreference')}</span>
      <select
        value={request.lengthPreference}
        on:change={(event) => update({
          lengthPreference: (event.currentTarget as HTMLSelectElement).value as SetupLengthPreference
        })}
      >
        <option value="auto">{label('setupLengthAuto')}</option>
        <option value="longer">{label('setupLengthLonger')}</option>
        <option value="shorter">{label('setupLengthShorter')}</option>
      </select>
      <small class="workspace-field-help">{label('setupLengthHelp')}</small>
    </label>
    <label class="workspace-field wide priority-field">
      <span>{label('setupMaxPieces')}</span>
      <input
        type="number"
        min="1"
        max="10"
        step="1"
        value={request.maxSetupPieces}
        on:input={(event) => update({
          maxSetupPieces: Number((event.currentTarget as HTMLInputElement).value)
        })}
      />
      <small class="workspace-field-help">{label('setupMaxPiecesHelp')}</small>
    </label>
    {#if cycle === 7}
      <div class="workspace-switch-row">
        <label class="workspace-switch-label">
          <input
            type="checkbox"
            checked={request.allowPostCycleBorrow}
            on:change={(event) => update({
              allowPostCycleBorrow: (event.currentTarget as HTMLInputElement).checked
            })}
          />
          <span class="workspace-switch" aria-hidden="true"></span>
          <span>{label('allowPostCycleBorrow')}</span>
        </label>
      </div>
      <small class="workspace-field-help boundary-help">{label('postCycleBorrowHelp')}</small>
    {/if}
  </section>

  {#if validationCodes.length}
    <ul class="workspace-validation">
      {#each validationCodes as code}<li>{label(code)}</li>{/each}
    </ul>
  {/if}
</WorkspaceControlPanel>

<style>
  .residue-facts {
    background: #f0f3f1;
    display: grid;
    font-size: 11px;
    gap: 1px;
    grid-template-columns: 1fr auto;
    margin-top: 14px;
  }
  .residue-facts span,
  .residue-facts strong { padding: 9px 10px; }
  .residue-facts span { color: #68736f; }
  .residue-facts strong { color: #173f3a; text-align: right; }
  .boundary-help { display: block; margin-top: 8px; }
  .priority-field { margin-top: 14px; }
  .queue-knowledge-field { margin-top: 14px; }
</style>
