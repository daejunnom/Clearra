<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import QueueTextInput from '../components/QueueTextInput.svelte';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  export let value = '';
  export let language: WorkspaceLanguage;
  export let qualifier = '';
  export let hint = '';
  export let maximumLength: number | undefined = undefined;
  const dispatch = createEventDispatcher<{ value: string }>();
  $: caption = workspaceMessage(language, 'queuePattern');
</script>
<label class="workspace-field wide supply-input">
  <span>{qualifier ? `${qualifier} · ${caption}` : caption}</span>
  <QueueTextInput class="workspace-queue-input" {value} maxlength={maximumLength}
    aria-label={qualifier ? `${qualifier} · ${caption}` : caption}
    placeholder={workspaceMessage(language, 'queuePlaceholder')} spellcheck="false"
    on:value={(event) => dispatch('value', event.detail)} />
</label>
{#if hint}<p class="workspace-field-help">{hint}</p>{/if}
<style>.supply-input { min-width: 0; } .supply-input span { overflow-wrap: anywhere; }</style>
