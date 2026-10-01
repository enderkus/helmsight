<script lang="ts">
  import { OctagonAlert, TriangleAlert, Info } from '@lucide/svelte';

  interface Props {
    severity: string;
  }
  let { severity }: Props = $props();
</script>

<span class={`sev sev-${severity}`}>
  {#if severity === 'critical'}<OctagonAlert size={14} aria-hidden="true" />
  {:else if severity === 'warning'}<TriangleAlert size={14} aria-hidden="true" />
  {:else}<Info size={14} aria-hidden="true" />{/if}
  <span>{severity === 'critical' ? 'Critical' : severity === 'warning' ? 'Warning' : 'Info'}</span>
</span>

<style>
  .sev {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
  }
  .sev-critical :global(svg) {
    color: var(--critical);
  }
  .sev-warning :global(svg) {
    color: var(--warning);
  }
  .sev-info :global(svg) {
    color: var(--unknown);
  }
</style>
