<script lang="ts">
  import type { Snippet } from 'svelte';
  interface Props {
    title: string;
    children?: Snippet;
  }
  let { title, children }: Props = $props();
</script>

<div class="empty">
  <p class="title">{title}</p>
  {#if children}<div class="muted">{@render children()}</div>{/if}
</div>

<style>
  .empty {
    padding: 32px 16px;
    text-align: center;
  }
  .title {
    font-weight: 600;
    margin-bottom: 4px;
  }
</style>
