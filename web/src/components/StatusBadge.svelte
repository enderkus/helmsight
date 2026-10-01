<script lang="ts">
  import { CircleCheck, TriangleAlert, OctagonAlert, CircleDashed, KeyRound, Unplug, Lock, CirclePause } from '@lucide/svelte';
  import { statusInfo } from '../lib/status';

  interface Props {
    status: string;
    compact?: boolean;
  }
  let { status, compact = false }: Props = $props();
  const info = $derived(statusInfo(status));
</script>

<span class={`badge tone-${info.tone}`} class:compact title={info.hint}>
  {#if status === 'ok'}<CircleCheck size={14} aria-hidden="true" />
  {:else if status === 'warning'}<TriangleAlert size={14} aria-hidden="true" />
  {:else if status === 'critical'}<OctagonAlert size={14} aria-hidden="true" />
  {:else if status === 'unreachable'}<Unplug size={14} aria-hidden="true" />
  {:else if status === 'auth_failed'}<Lock size={14} aria-hidden="true" />
  {:else if status === 'host_key_unknown' || status === 'host_key_changed'}<KeyRound size={14} aria-hidden="true" />
  {:else if status === 'disabled'}<CirclePause size={14} aria-hidden="true" />
  {:else}<CircleDashed size={14} aria-hidden="true" />{/if}
  <span class="label" class:sr-only={compact}>{info.label}</span>
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text);
    white-space: nowrap;
  }
  .badge :global(svg) {
    flex: none;
  }
  .tone-ok :global(svg) {
    color: var(--ok);
  }
  .tone-warning :global(svg) {
    color: var(--warning);
  }
  .tone-critical :global(svg) {
    color: var(--critical);
  }
  .tone-unknown :global(svg) {
    color: var(--unknown);
  }
</style>
