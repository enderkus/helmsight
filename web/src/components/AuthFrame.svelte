<script lang="ts">
  import type { Snippet } from 'svelte';
  import { PRODUCT_NAME } from '../lib/brand';
  interface Props {
    title: string;
    children: Snippet;
  }
  let { title, children }: Props = $props();
</script>

<main class="auth">
  <div class="card panel">
    <div class="brand">
      <span class="logo" aria-hidden="true"></span>
      <span>{PRODUCT_NAME}</span>
    </div>
    <h1>{title}</h1>
    {@render children()}
  </div>
</main>

<style>
  .auth {
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px 16px;
  }
  .card {
    width: 100%;
    max-width: 360px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    color: var(--text-2);
  }
  .logo {
    width: 16px;
    height: 16px;
    border: 2px solid var(--accent);
    border-radius: 50%;
  }
  h1 {
    font-size: 16px;
  }
</style>
