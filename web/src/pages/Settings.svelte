<script lang="ts">
  import PasswordForm from '../components/PasswordForm.svelte';
  import TotpEnroll from '../components/TotpEnroll.svelte';
  import { api, ApiError } from '../lib/api';
  import { session, loadSession } from '../lib/session.svelte';
  import { theme, setTheme, type ThemePref } from '../lib/theme.svelte';
  import { toast } from '../lib/toast.svelte';
  import { datetime } from '../lib/format';

  interface Props {
    params?: Record<string, string>;
  }
  let { params: _params = {} }: Props = $props();

  let disablePw = $state('');
  let disableCode = $state('');
  let busy = $state(false);
  const user = $derived(session.user);
  const sso = $derived(!!user?.oidc_subject);

  async function disableTotp(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      await api.post('/auth/totp/disable', { password: disablePw, code: disableCode });
      disablePw = disableCode = '';
      toast('Two-factor authentication disabled.');
      await loadSession();
    } catch (err) {
      toast(err instanceof ApiError ? err.message : 'Could not disable.', 'error');
    } finally {
      busy = false;
    }
  }
</script>

<div class="page narrow">
  <div class="page-header"><h1>Account</h1></div>
  <div class="stack">
    <section class="panel">
      <header class="panel-header"><h2>Profile</h2></header>
      <div class="panel-body">
        <dl class="props">
          <dt>User name</dt><dd class="mono">{user?.username}</dd>
          <dt>Role</dt><dd>{user?.role}</dd>
          <dt>Sign-in</dt><dd>{sso ? 'Single sign-on' : 'Local password'}</dd>
          <dt>Last sign-in</dt><dd>{datetime(user?.last_login_at)}</dd>
        </dl>
      </div>
    </section>

    <section class="panel">
      <header class="panel-header"><h2>Appearance</h2></header>
      <div class="panel-body">
        <div class="segmented" role="group" aria-label="Theme">
          {#each [['system', 'System'], ['light', 'Light'], ['dark', 'Dark']] as [v, label] (v)}
            <button type="button" aria-pressed={theme.pref === v} onclick={() => setTheme(v as ThemePref)}>{label}</button>
          {/each}
        </div>
      </div>
    </section>

    {#if !sso}
      <section class="panel">
        <header class="panel-header"><h2>Password</h2></header>
        <div class="panel-body"><PasswordForm ondone={() => toast('Password changed. Other sessions were signed out.')} /></div>
      </section>

      <section class="panel">
        <header class="panel-header">
          <h2>Two-factor authentication</h2>
          <span class="spacer"></span>
          <span class="secondary">{user?.totp_enabled ? 'Enabled' : 'Not enabled'}</span>
        </header>
        <div class="panel-body stack">
          {#if user?.totp_enabled}
            <form class="stack" onsubmit={disableTotp}>
              <p class="secondary">To disable, confirm your password and a current code.</p>
              <div class="row wrap">
                <label class="field"><span>Password</span><input class="input" type="password" autocomplete="current-password" bind:value={disablePw} required /></label>
                <label class="field"><span>Code</span><input class="input mono" inputmode="numeric" maxlength="7" bind:value={disableCode} required /></label>
              </div>
              <div><button class="btn danger" type="submit" disabled={busy}>Disable</button></div>
            </form>
          {:else}
            <TotpEnroll ondone={() => (toast('Two-factor authentication enabled.'), void loadSession())} />
          {/if}
        </div>
      </section>
    {/if}
  </div>
</div>

<style>
  .narrow {
    max-width: 760px;
  }
  .wrap {
    flex-wrap: wrap;
  }
</style>
