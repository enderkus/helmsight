<script lang="ts">
  import AuthFrame from '../components/AuthFrame.svelte';
  import { api, ApiError } from '../lib/api';
  import { setUser, session } from '../lib/session.svelte';
  import { router } from '../lib/router.svelte';
  import { PRODUCT_NAME } from '../lib/brand';
  import type { User } from '../lib/types';

  // The token is passed in the URL fragment so it never reaches server logs.
  const fromHash = new URLSearchParams(location.hash.slice(1)).get('token') ?? '';
  let token = $state(fromHash);
  let username = $state('admin');
  let password = $state('');
  let confirm = $state('');
  let busy = $state(false);
  let error = $state('');

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    error = '';
    if (password !== confirm) {
      error = 'Passwords do not match.';
      return;
    }
    busy = true;
    try {
      const r = await api.post<{ user: User; csrf: string }>('/auth/setup', { token, username, password });
      history.replaceState(null, '', '/');
      if (session.meta) session.meta.setup_required = false;
      setUser(r.user, r.csrf);
      router.navigate('/', { replace: true });
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Setup failed.';
    } finally {
      busy = false;
    }
  }
</script>

<AuthFrame title="Create the first administrator">
  <p class="secondary">
    No users exist yet. Use the setup token that {PRODUCT_NAME} printed when it started.
  </p>
  {#if error}<p class="banner critical" role="alert">{error}</p>{/if}
  <form class="stack" onsubmit={submit}>
    {#if !fromHash}
      <label class="field"><span>Setup token</span><input class="input mono" bind:value={token} required autocomplete="off" /></label>
    {/if}
    <label class="field"><span>User name</span><input class="input" bind:value={username} autocomplete="username" required /></label>
    <label class="field">
      <span>Password</span>
      <input class="input" type="password" bind:value={password} autocomplete="new-password" minlength="12" required />
      <span class="hint">At least 12 characters.</span>
    </label>
    <label class="field"><span>Repeat password</span><input class="input" type="password" bind:value={confirm} autocomplete="new-password" required /></label>
    <button class="btn primary" type="submit" disabled={busy}>{busy ? 'Creating…' : 'Create administrator'}</button>
  </form>
  {#if session.meta?.features.oidc}
    <p class="muted or">or</p>
    <a class="btn" href="/api/v1/auth/oidc/start" rel="external">{session.meta.features.oidc}</a>
  {/if}
</AuthFrame>

<style>
  .or {
    text-align: center;
    font-size: 12px;
  }
</style>
