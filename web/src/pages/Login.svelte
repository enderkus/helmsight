<script lang="ts">
  import { LogIn, KeySquare } from '@lucide/svelte';
  import AuthFrame from '../components/AuthFrame.svelte';
  import { api, ApiError } from '../lib/api';
  import { session, setUser } from '../lib/session.svelte';
  import { router } from '../lib/router.svelte';
  import type { User } from '../lib/types';

  let username = $state('');
  let password = $state('');
  let totp = $state('');
  let needTotp = $state(false);
  let busy = $state(false);
  let error = $state(router.query.get('error') ?? '');
  let totpInput = $state<HTMLInputElement | undefined>();

  const features = $derived(session.meta?.features);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = '';
    try {
      const r = await api.post<{ user: User; csrf: string }>('/auth/login', {
        username,
        password,
        totp: needTotp ? totp : undefined,
      });
      setUser(r.user, r.csrf);
      if (router.path === '/login' || router.path === '/setup') router.navigate('/', { replace: true });
    } catch (err) {
      if (err instanceof ApiError && err.code === 'totp_required') {
        needTotp = true;
        queueMicrotask(() => totpInput?.focus());
      } else {
        error = err instanceof ApiError ? err.message : 'Sign-in failed.';
        if (err instanceof ApiError && err.code === 'invalid_totp') totp = '';
      }
    } finally {
      busy = false;
    }
  }
</script>

<AuthFrame title="Sign in">
  {#if error}<p class="banner critical" role="alert">{error}</p>{/if}
  {#if features?.local_login !== false}
    <form onsubmit={submit} class="stack">
      {#if !needTotp}
        <label class="field">
          <span>User name</span>
          <input class="input" name="username" autocomplete="username" bind:value={username} required />
        </label>
        <label class="field">
          <span>Password</span>
          <input class="input" type="password" name="password" autocomplete="current-password" bind:value={password} required />
        </label>
      {:else}
        <label class="field">
          <span>Authentication code</span>
          <input
            class="input mono"
            bind:this={totpInput}
            inputmode="numeric"
            autocomplete="one-time-code"
            pattern="[0-9 ]*"
            maxlength="7"
            bind:value={totp}
            required
          />
          <span class="hint">Enter the six-digit code from your authenticator app.</span>
        </label>
      {/if}
      <button class="btn primary" type="submit" disabled={busy}><LogIn size={15} />{busy ? 'Signing in…' : 'Sign in'}</button>
      {#if needTotp}
        <button class="btn ghost small" type="button" onclick={() => ((needTotp = false), (totp = ''))}>Back</button>
      {/if}
    </form>
  {/if}
  {#if features?.oidc}
    <a class="btn" href="/api/v1/auth/oidc/start" rel="external"><KeySquare size={15} />{features.oidc}</a>
  {/if}
</AuthFrame>
