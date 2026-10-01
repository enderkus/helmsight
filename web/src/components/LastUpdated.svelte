<script lang="ts">
  import { Clock } from '@lucide/svelte';
  import { clock } from '../lib/clock.svelte';
  import { ago, datetime } from '../lib/format';

  interface Props {
    ts: number | null;
    stale?: boolean;
    prefix?: string;
  }
  let { ts, stale = false, prefix = 'Updated' }: Props = $props();
</script>

<span class="updated" class:is-stale={stale} title={ts ? datetime(ts) : 'No data yet'}>
  <Clock size={12} aria-hidden="true" />
  {prefix}
  {ago(ts, clock.now)}{#if stale}<span class="stale-label">stale</span>{/if}
</span>

<style>
  .updated {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text-3);
    font-size: 12px;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .stale-label {
    margin-left: 4px;
    padding: 0 5px;
    border: 1px solid var(--warning);
    border-radius: var(--radius-sm);
    color: var(--text);
    background: var(--warning-wash);
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }
</style>
