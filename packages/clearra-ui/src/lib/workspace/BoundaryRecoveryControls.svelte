<script lang="ts">
  import { Database, Gauge } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';
  import QueueTextInput from '../components/QueueTextInput.svelte';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import { boundaryRecoveryBagSlots, updateRecoveryPlacements, type BoundaryRecoveryRequest } from './boundaryRecoveryModel';
  import { componentMessage, type ComponentMessageKey } from '../i18n/componentCatalog';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let request: BoundaryRecoveryRequest;
  export let language: WorkspaceLanguage;
  export let validation: string[] = [];
  export let selectedRolePosition = 1;
  const dispatch = createEventDispatcher<{ change: BoundaryRecoveryRequest; role: number }>();
  const rules = ['srs-plus', 'srs', 'srs-x', 'jstris-180'] as const;
  const spins = ['t-spins', 't-spins-plus', 'all-spin', 'all-spin-plus', 'all-mini', 'all-mini-plus'] as const;
  let savedRoles: bigint[] = [];
  $: label = (key: ComponentMessageKey) => componentMessage(language, key);
  $: standard = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: bagSlots = boundaryRecoveryBagSlots(request.stageOneCount, request.placements);
  $: roleCount = Number.isInteger(request.placements) && request.placements >= 2 && request.placements <= 42 ? request.placements : 0;
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
        spellcheck="false" on:value={(event) => patch({ queue: event.detail })} />
    </label>
    <div class="workspace-field-grid">
      <label class="workspace-field"><span>{label('recoveryStageOneCount')}</span>
        <input type="number" min="1" max="41" value={request.stageOneCount}
          on:input={(event) => patch({ stageOneCount: (event.currentTarget as HTMLInputElement).valueAsNumber, preserveB2BBags: [] })} />
      </label>
      <label class="workspace-field"><span>{label('recoveryPlacements')}</span>
        <input type="number" min="2" max="42" value={request.placements}
          on:input={(event) => dispatch('change', updateRecoveryPlacements(request, (event.currentTarget as HTMLInputElement).valueAsNumber))} />
      </label>
    </div>
    <div class="workspace-switch-row"><label class="workspace-switch-label">
      <input type="checkbox" checked={request.holdEnabled} on:change={(event) => patch({ holdEnabled: (event.currentTarget as HTMLInputElement).checked })} />
      <span class="workspace-switch" aria-hidden="true"></span><span>{standard('hold')}</span>
    </label></div>
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
      <label class="workspace-field"><span>{standard('rule')}</span>
        <select value={request.rule} on:change={(event) => patch({ rule: (event.currentTarget as HTMLSelectElement).value as BoundaryRecoveryRequest['rule'] })}>
          {#each rules as rule}<option value={rule}>{rule}</option>{/each}
        </select>
      </label>
      <label class="workspace-field"><span>{standard('spinProfile')}</span>
        <select value={request.spinProfile} on:change={(event) => patch({ spinProfile: (event.currentTarget as HTMLSelectElement).value as BoundaryRecoveryRequest['spinProfile'] })}>
          {#each spins as spin}<option value={spin}>{spin}</option>{/each}
        </select>
      </label>
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
          <input type="number" min={request.stageOneCount + 1} max={request.placements} value={request.borrowRolePosition}
            on:input={(event) => patch({ borrowRolePosition: (event.currentTarget as HTMLInputElement).valueAsNumber })} />
        </label>
      {/if}
    </div>
    <label class="workspace-field wide"><span>{label('recoveryQueuePattern')}</span>
      <QueueTextInput class="workspace-queue-input" value={request.queuePattern} placeholder="IJLOSTZP7" spellcheck="false"
        on:value={(event) => patch({ queuePattern: event.detail })} />
    </label>
    <p class="workspace-field-help">{label('recoveryScope')}</p>
  </section>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading">{label('recoveryBudget')}</h2>
    <label class="workspace-field"><span>{label('recoveryMaxStates')}</span>
      <input type="number" min="1" max="1000000" value={request.maxStates}
        on:input={(event) => patch({ maxStates: (event.currentTarget as HTMLInputElement).valueAsNumber })} />
    </label>
    {#if request.queuePattern.trim()}
      <div class="workspace-field-grid">
        <label class="workspace-field"><span>{label('recoveryPatternEvaluations')}</span>
          <input type="number" min="1" max="100000" value={request.maxPatternEvaluations}
            on:input={(event) => patch({ maxPatternEvaluations: (event.currentTarget as HTMLInputElement).valueAsNumber })} />
        </label>
        <label class="workspace-field"><span>{label('recoveryTotalStates')}</span>
          <input type="number" min="1" max="100000000" value={request.maxTotalStates}
            on:input={(event) => patch({ maxTotalStates: (event.currentTarget as HTMLInputElement).valueAsNumber })} />
        </label>
      </div>
    {/if}
  </section>
  {#if validation.length > 0}<div class="workspace-validation" role="alert"><p>{label('recoveryInvalid')}</p></div>{/if}
</WorkspaceControlPanel>
