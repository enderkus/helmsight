<script lang="ts">
  import { Info } from '@lucide/svelte';
  interface Props {
    reason: string;
  }
  let { reason }: Props = $props();
</script>

<p class="na"><Info size={14} aria-hidden="true" /><span><strong>n/a</strong> — {reason}</span></p>

<style>
  .na {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 12px;
    color: var(--text-2);
    font-size: 12.5px;
  }
  .na :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--unknown);
  }
</style>
