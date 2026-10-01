<script lang="ts">
  import { CalendarRange } from '@lucide/svelte';
  import { PRESETS, fromLocalInput, toLocalInput, type Range } from '../lib/timerange';

  interface Props {
    value: Range;
    onchange: (r: Range) => void;
  }
  let { value, onchange }: Props = $props();

  let custom = $state(false);
  let from = $state('');
  let to = $state('');
  let error = $state('');

  function openCustom() {
    custom = !custom;
    from = toLocalInput(value.from);
    to = toLocalInput(value.to);
    error = '';
  }

  function apply(e: SubmitEvent) {
    e.preventDefault();
    const f = fromLocalInput(from);
    const t = fromLocalInput(to);
    if (f === null || t === null || t <= f) {
      error = 'The end must be after the start.';
      return;
    }
    custom = false;
    onchange({ key: 'custom', from: f, to: t, live: false });
  }

  function pick(key: string) {
    custom = false;
    const p = PRESETS.find((x) => x.key === key);
    if (!p) return;
    const now = Math.floor(Date.now() / 1000);
    onchange({ key, from: now - p.secs, to: now, live: true });
  }
</script>

<div class="picker">
  <div class="segmented" role="group" aria-label="Time range">
    {#each PRESETS as p (p.key)}
      <button type="button" aria-pressed={value.key === p.key} title={p.label} onclick={() => pick(p.key)}>{p.key}</button>
    {/each}
    <button type="button" aria-pressed={value.key === 'custom'} aria-expanded={custom} onclick={openCustom}>
      <CalendarRange size={13} aria-hidden="true" /> Custom
    </button>
  </div>
  {#if custom}
    <form class="custom panel" onsubmit={apply}>
      <label class="field"><span>From</span><input class="input" type="datetime-local" bind:value={from} required /></label>
      <label class="field"><span>To</span><input class="input" type="datetime-local" bind:value={to} required /></label>
      {#if error}<p class="err" role="alert">{error}</p>{/if}
      <div class="row">
        <button class="btn primary small" type="submit">Apply</button>
        <button class="btn small" type="button" onclick={() => (custom = false)}>Cancel</button>
      </div>
    </form>
  {/if}
</div>

<style>
  .picker {
    position: relative;
  }
  .custom {
    position: absolute;
    right: 0;
    top: 32px;
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    width: 250px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.14);
  }
  .err {
    color: var(--critical-text);
    font-size: 12px;
  }
</style>
