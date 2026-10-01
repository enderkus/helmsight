<script lang="ts">
  interface Props {
    rows?: number;
    height?: number;
  }
  let { rows = 6, height = 20 }: Props = $props();
</script>

<div class="rows" aria-busy="true" aria-label="Loading">
  {#each Array.from({ length: rows }, (_, i) => i) as i (i)}
    <div class="skeleton" style:height={`${height}px`} style:width={`${92 - (i % 3) * 14}%`}></div>
  {/each}
</div>

<style>
  .rows {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
  }
</style>
