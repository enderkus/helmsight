<script lang="ts">
  import Dialog from './Dialog.svelte';
  import { SHORTCUTS } from '../lib/keyboard';

  interface Props {
    open: boolean;
    onclose: () => void;
  }
  let { open, onclose }: Props = $props();
</script>

<Dialog {open} {onclose} title="Keyboard shortcuts" width={380}>
  <table class="keys">
    <tbody>
      {#each SHORTCUTS as [keys, what] (keys)}
        <tr>
          <td>
            {#each keys.split(' ') as k, i (i)}{#if i > 0}<span class="then">then</span>{/if}<kbd>{k}</kbd>{/each}
          </td>
          <td>{what}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</Dialog>

<style>
  .keys {
    border-collapse: collapse;
    width: 100%;
  }
  .keys td {
    padding: 5px 0;
    border-bottom: 1px solid var(--border);
  }
  .keys td:first-child {
    width: 120px;
    white-space: nowrap;
  }
  .then {
    margin: 0 4px;
    color: var(--text-3);
    font-size: 11px;
  }
</style>
