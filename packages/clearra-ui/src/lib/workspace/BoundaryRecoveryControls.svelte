<script lang="ts">
  import SpinProfileSelect from './SpinProfileSelect.svelte';
  import RuleProfileSelect from './RuleProfileSelect.svelte';
  import WorkspaceToggle from './WorkspaceToggle.svelte';
  import { Database, Gauge } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';
  import QueuePatternHelp from './QueuePatternHelp.svelte';
  import QueueTextInput from '../components/QueueTextInput.svelte';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import { boundaryRecoveryBagSlots, recoveryPlacementHorizon, updateRecoveryQueue, type BoundaryRecoveryRequest } from './boundaryRecoveryModel';
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
  $: bagSlots = boundaryRecoveryBagSlots(request.stageOneCount, horizon);
  $: roleCount = Number.isInteger(horizon) && horizon >= 2 && horizon <= 42 ? horizon : 0;
  function patch(value: Partial<BoundaryRecoveryRequest>) { dispatch('change', { ...request, ...value }); }
  function exactRoles(enabled: boolean) {
    if (!enabled) savedRoles = request.placementRoleMasks;
    patch({ placementRoleMasks: enabled ? Array.from({ length: roleCount }, (_, i) => savedRoles[i] ?? 0n) : [] });
  }
  function bagB2b(position: number, enabled: boolean) {
    const bags = new Set(request.preserveB2BBags);
    if (enabled) bags.add(position); else bags.delete(position);
    patch({ preserveB2BBags: [...bags].sort((a,b) => a-b) });
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
    <span class="workspace-field-label">{label('recoveryMaxEarlyPlacements')}</span>
    <div class="workspace-segmented two" role="group" aria-label={label('recoveryMaxEarlyPlacements')}>
      {#each [0, 1] as count}
        <button type="button" class:active={request.maxEarlyPlacements === count} aria-pressed={request.maxEarlyPlacements === count}
          on:click={() => patch({ maxEarlyPlacements: count as 0 | 1 })}>{label(count === 0 ? 'recoveryNoEarly' : 'recoveryOneEarly')}</button>
      {/each}
    </div>
    <div class="workspace-field-grid">
      <RuleProfileSelect value={request.rule} {language}
        on:change={(event) => patch({ rule: event.detail })} />
      <SpinProfileSelect value={request.spinProfile} {language}
        on:change={(event) => patch({ spinProfile: event.detail })} />
    </div>
  </section>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading">{label('recoveryB2b')}</h2>
    <div class="workspace-toggle-grid">
      <label class="workspace-switch-label"><input type="checkbox" checked={request.initialB2B}
        on:change={(event) => patch({ initialB2B: (event.currentTarget as HTMLInputElement).checked })} />
        <span class="workspace-switch" aria-hidden="true"></span><span>{label('recoveryInitialB2b')}</span>
      </label>
      {#each bagSlots as bag}
        <label class="workspace-switch-label"><input type="checkbox" checked={request.preserveB2BBags.includes(bag.position)}
          on:change={(event) => bagB2b(bag.position, (event.currentTarget as HTMLInputElement).checked)} />
          <span class="workspace-switch" aria-hidden="true"></span><span>{label(bag.stage === 1 ? 'recoveryPreserveStageOne' : 'recoveryPreserveStageTwo')} · {label('recoveryBagUnit')} {bag.stageBag}</span>
        </label>
      {/each}
    </div>
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
      {#if request.maxEarlyPlacements === 1}
        <label class="workspace-field"><span>{label('recoveryBorrowPosition')}</span>
          <input type="number" min={request.stageOneCount + 1} max={horizon} value={request.borrowRolePosition}
            on:input={(event) => patch({ borrowRolePosition: (event.currentTarget as HTMLInputElement).valueAsNumber })} />
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
