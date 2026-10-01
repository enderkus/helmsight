<script lang="ts">
  import { CircleCheck, CircleAlert, Info, X } from '@lucide/svelte';
  import { dismiss, toasts } from '../lib/toast.svelte';
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each toasts.list as t (t.id)}
    <div class={`toast ${t.kind}`}>
      {#if t.kind === 'ok'}<CircleCheck size={16} aria-hidden="true" />
      {:else if t.kind === 'error'}<CircleAlert size={16} aria-hidden="true" />
      {:else}<Info size={16} aria-hidden="true" />{/if}
      <span class="text">{t.text}</span>
      <button class="btn ghost icon small" type="button" aria-label="Dismiss" onclick={() => dismiss(t.id)}><X size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: min(420px, calc(100vw - 32px));
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 12px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.16);
  }
  .text {
    flex: 1;
  }
  .ok :global(svg:first-child) {
    color: var(--ok);
  }
  .error :global(svg:first-child) {
    color: var(--critical);
  }
  .info :global(svg:first-child) {
    color: var(--accent);
  }
</style>
