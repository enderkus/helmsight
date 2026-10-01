<script lang="ts">
  import type { Snippet } from 'svelte';
  import { X } from '@lucide/svelte';

  interface Props {
    open: boolean;
    title: string;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
    width?: number;
  }
  let { open, title, onclose, children, footer, width = 480 }: Props = $props();
  let el = $state<HTMLDialogElement | undefined>();

  $effect(() => {
    if (!el) return;
    if (open && !el.open) el.showModal();
    if (!open && el.open) el.close();
  });
</script>

<dialog
  bind:this={el}
  aria-labelledby="dlg-title"
  style:width={`min(${width}px, calc(100vw - 32px))`}
  onclose={onclose}
  onclick={(e) => {
    if (e.target === el) onclose();
  }}
>
  {#if open}
    <header>
      <h2 id="dlg-title">{title}</h2>
      <button class="btn ghost icon small" type="button" onclick={onclose} aria-label="Close">
        <X size={16} />
      </button>
    </header>
    <div class="content">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  {/if}
</dialog>

<style>
  dialog {
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.25);
  }
  dialog::backdrop {
    background: var(--overlay);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px 10px 16px;
    border-bottom: 1px solid var(--border);
  }
  .content {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 10px 16px;
    border-top: 1px solid var(--border);
    background: var(--surface-2);
  }
</style>
