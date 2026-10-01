<script lang="ts">
  import EmptyState from '../components/EmptyState.svelte';
  interface Props {
    params?: Record<string, string>;
  }
  let { params: _params = {} }: Props = $props();
</script>

<div class="page">
  <EmptyState title="Page not found">
    <p>The address does not match any page. <a href="/">Go to the fleet overview</a>.</p>
  </EmptyState>
</div>
