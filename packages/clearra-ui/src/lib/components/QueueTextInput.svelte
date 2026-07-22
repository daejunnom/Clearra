<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';

  export let value = '';

  const dispatch = createEventDispatcher<{ value: string }>();
  let input: HTMLInputElement;
  let lastExternalValue = value;
  let pendingValue: string | null = null;

  $: if (value !== lastExternalValue) {
    lastExternalValue = value;
    if (pendingValue === value) {
      pendingValue = null;
    } else {
      pendingValue = null;
      if (input && input.value !== value) input.value = value;
    }
  }

  function handleInput(event: Event) {
    const target = event.currentTarget as HTMLInputElement;
    const raw = target.value;
    const selectionStart = target.selectionStart;
    const selectionEnd = target.selectionEnd;
    const normalized = raw.toUpperCase();
    pendingValue = normalized;
    if (normalized !== raw) {
      target.value = normalized;
      if (selectionStart !== null && selectionEnd !== null) {
        target.setSelectionRange(
          raw.slice(0, selectionStart).toUpperCase().length,
          raw.slice(0, selectionEnd).toUpperCase().length
        );
      }
    }
    dispatch('value', normalized);
  }

  onMount(() => {
    input.value = value;
    lastExternalValue = value;
  });
</script>

<input {...$$restProps} bind:this={input} on:input={handleInput} />
