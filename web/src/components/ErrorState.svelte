<script lang="ts">
  import { CircleAlert } from '@lucide/svelte';
  interface Props {
    message: string;
    onretry?: () => void;
  }
  let { message, onretry }: Props = $props();
</script>

<div class="banner critical" role="alert">
  <CircleAlert size={16} aria-hidden="true" />
  <div class="body">
    <p class="title">Could not load data</p>
    <p>{message}</p>
  </div>
  {#if onretry}<button class="btn small" type="button" onclick={onretry}>Retry</button>{/if}
</div>
