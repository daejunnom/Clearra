<script lang="ts">
  import SpinProfileSelect from './SpinProfileSelect.svelte';
  import RuleProfileSelect from './RuleProfileSelect.svelte';
  import WorkspaceToggle from './WorkspaceToggle.svelte';
  import { Database, Gauge } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';
  import QueuePatternHelp from './QueuePatternHelp.svelte';
  import QueueTextInput from '../components/QueueTextInput.svelte';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import { availableEarlyPlacementCount, recoveryPlacementHorizon, updateRecoveryQueue, type BoundaryRecoveryRequest } from './boundaryRecoveryModel';
  import { componentMessage, type ComponentMessageKey } from '../i18n/componentCatalog';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let request: BoundaryRecoveryRequest;
  export let language: WorkspaceLanguage;
  export let validation: string[] = [];
  export let selectedRolePosition = 1;
  const dispatch = createEventDispatcher<{ change: BoundaryRecoveryRequest; role: number }>();
  let savedRoles: bigint[] = [];
  $: label = (key: ComponentMessageKey) => componentMessage(language, key);
  $: standard = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: horizon = recoveryPlacementHorizon(request);
  $: earlyLimit = availableEarlyPlacementCount(request);
  $: earlyOptions = Array.from({ length: earlyLimit + 1 }, (_, count) => count);
  $: roleCount = Number.isInteger(horizon) && horizon >= 2 && horizon <= 42 ? horizon : 0;
  function patch(value: Partial<BoundaryRecoveryRequest>) { dispatch('change', { ...request, ...value }); }
  function exactRoles(enabled: boolean) {
    if (!enabled) savedRoles = request.placementRoleMasks;
    patch({ placementRoleMasks: enabled ? Array.from({ length: roleCount }, (_, i) => savedRoles[i] ?? 0n) : [] });
  }
</script>

<WorkspaceControlPanel ariaLabel={label('boundaryRecovery')}>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading"><Database size={16} strokeWidth={1.8} />{standard('source')}</h2>
    <label class="workspace-field"><span>{label('recoveryQueue')}</span>
      <QueueTextInput class="workspace-queue-input" value={request.queue} maxlength="42" placeholder="IOTSZJL"
        spellcheck="false" on:value={(event) => dispatch('change', updateRecoveryQueue(request, event.detail))} />
    </label>
    <div class="workspace-field-grid">
      <label class="workspace-field"><span>{label('recoveryStageOneCount')}</span>
        <input type="number" min="1" max="41" value={request.stageOneCount}
          on:input={(event) => patch({ stageOneCount: (event.currentTarget as HTMLInputElement).valueAsNumber, preserveB2BBags: [] })} />
      </label>
      <div class="workspace-field recovery-required-pieces" role="status">
        <span>{standard('piecesNeeded')}</span>
        <output>{request.placements ?? (request.placementRoleMasks.length || label('recoveryAutomaticPieces'))}</output>
        <small class="workspace-field-help">{label('recoveryAutomaticPiecesHelp')}</small>
      </div>
    </div>
    <div class="workspace-switch-row"><WorkspaceToggle label={standard('hold')} checked={request.holdEnabled}
        on:change={(event) => patch({ holdEnabled: event.detail })} /></div>
  </section>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading"><Gauge size={16} strokeWidth={1.8} />{standard('search')}</h2>
    <label class="workspace-field">
      <span>{label('recoveryMaxEarlyPlacements')}</span>
      <select aria-label={label('recoveryMaxEarlyPlacements')} aria-description={label('recoveryEarlyCountHelp')}
        value={request.maxEarlyPlacements} aria-invalid={!earlyOptions.includes(request.maxEarlyPlacements)}
        disabled={earlyLimit === 0}
        on:change={(event) => patch({ maxEarlyPlacements: Number(event.currentTarget.value) })}>
        {#if !earlyOptions.includes(request.maxEarlyPlacements)}
          <option value={request.maxEarlyPlacements} disabled>{request.maxEarlyPlacements}</option>
        {/if}
        {#each earlyOptions as count}<option value={count}>{count}</option>{/each}
      </select>
      <small class="workspace-field-help">{label('recoveryEarlyCountHelp')}</small>
    </label>
    <div class="workspace-field-grid">
      <RuleProfileSelect value={request.rule} {language}
        on:change={(event) => patch({ rule: event.detail })} />
      <SpinProfileSelect value={request.spinProfile} {language}
        on:change={(event) => patch({ spinProfile: event.detail })} />
    </div>
  </section>
  <section class="workspace-control-section">
    <WorkspaceToggle label={standard('preserveB2B')} checked={request.preserveB2B ?? false}
      on:change={(event) => patch({ preserveB2B: event.detail })} />
    <p class="workspace-field-help">{label('recoveryB2bReplayHelp')}</p>
  </section>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading">{label('recoveryAdvanced')}</h2>
    <label class="workspace-switch-label"><input type="checkbox" checked={request.placementRoleMasks.length > 0}
      disabled={roleCount === 0} on:change={(event) => exactRoles((event.currentTarget as HTMLInputElement).checked)} />
      <span class="workspace-switch" aria-hidden="true"></span><span>{label('recoveryExactRoles')}</span>
    </label>
    <div class="workspace-field-grid">
      {#if request.placementRoleMasks.length > 0}
        <label class="workspace-field"><span>{label('recoveryRolePosition')}</span>
          <select value={selectedRolePosition} on:change={(event) => dispatch('role', Number((event.currentTarget as HTMLSelectElement).value))}>
            {#each request.placementRoleMasks as _, i}<option value={i + 1}>{i + 1} · {request.queue.trim()[i]?.toUpperCase() ?? '?'}</option>{/each}
          </select>
        </label>
      {/if}

    </div>
    <label class="workspace-field wide"><span>{label('recoveryQueuePattern')}</span>
      <QueueTextInput class="workspace-queue-input" value={request.queuePattern} placeholder="IJLOSTZP7" spellcheck="false"
        on:value={(event) => patch({ queuePattern: event.detail })} />
    </label>
    <QueuePatternHelp {language} />
    <p class="workspace-field-help">{label('recoveryPatternReferenceHelp')}</p>
    <p class="workspace-field-help">{label('recoveryScope')}</p>
  </section>
  {#if request.queuePattern.trim()}
    <section class="workspace-control-section">
      <h2 class="workspace-control-heading">{label('recoveryBudget')}</h2>
      <label class="workspace-field"><span>{label('recoveryPatternEvaluations')}</span>
        <input type="number" min="1" max="100000" value={request.maxPatternEvaluations}
          on:input={(event) => patch({ maxPatternEvaluations: (event.currentTarget as HTMLInputElement).valueAsNumber })} />
      </label>
    </section>
  {/if}
  {#if validation.length > 0}<div class="workspace-validation" role="alert"><p>{label('recoveryInvalid')}</p></div>{/if}
</WorkspaceControlPanel>
