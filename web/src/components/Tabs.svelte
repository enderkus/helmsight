<script lang="ts">
  interface Tab {
    id: string;
    label: string;
    count?: number | null;
    alert?: boolean;
  }
  interface Props {
    tabs: Tab[];
    active: string;
    onselect: (id: string) => void;
    label: string;
  }
  let { tabs, active, onselect, label }: Props = $props();

  function key(e: KeyboardEvent, i: number) {
    let n = -1;
    if (e.key === 'ArrowRight') n = (i + 1) % tabs.length;
    else if (e.key === 'ArrowLeft') n = (i - 1 + tabs.length) % tabs.length;
    else if (e.key === 'Home') n = 0;
    else if (e.key === 'End') n = tabs.length - 1;
    if (n >= 0) {
      e.preventDefault();
      const t = tabs[n];
      if (t) {
        onselect(t.id);
        document.getElementById(`tab-${t.id}`)?.focus();
      }
    }
  }
</script>

<div class="tabs" role="tablist" aria-label={label}>
  {#each tabs as t, i (t.id)}
    <button
      id={`tab-${t.id}`}
      role="tab"
      type="button"
      aria-selected={t.id === active}
      tabindex={t.id === active ? 0 : -1}
      onclick={() => onselect(t.id)}
      onkeydown={(e) => key(e, i)}
    >
      {t.label}
      {#if t.count !== undefined && t.count !== null}
        <span class="count" class:alert={t.alert}>{t.count}</span>
      {/if}
    </button>
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    gap: 2px;
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
    margin-bottom: 12px;
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 12px;
    border: 0;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    background: none;
    color: var(--text-2);
    font: inherit;
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
  }
  button:hover {
    color: var(--text);
  }
  button[aria-selected='true'] {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: 9px;
    background: var(--surface-3);
    color: var(--text-2);
    font-size: 11px;
    line-height: 17px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .count.alert {
    background: var(--critical-wash);
    color: var(--critical-text);
    font-weight: 600;
  }
</style>
