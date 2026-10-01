<script lang="ts">
  import { api, ApiError } from '../lib/api';

  interface Props {
    ondone: () => void;
  }
  let { ondone }: Props = $props();
  let setup = $state<{ secret: string; otpauth_url: string; qr: string } | null>(null);
  let code = $state('');
  let busy = $state(false);
  let error = $state('');

  async function start() {
    busy = true;
    error = '';
    try {
      setup = await api.post('/auth/totp/setup');
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Could not start enrolment.';
    } finally {
      busy = false;
    }
  }

  async function confirm(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = '';
    try {
      await api.post('/auth/totp/enable', { code });
      ondone();
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Could not enable two-factor authentication.';
    } finally {
      busy = false;
    }
  }
</script>

{#if error}<p class="banner critical" role="alert">{error}</p>{/if}
{#if !setup}
  <p class="secondary">Use an authenticator app (TOTP) to add a second factor to password sign-in.</p>
  <div><button class="btn primary" type="button" disabled={busy} onclick={start}>Set up authenticator</button></div>
{:else}
  <div class="enroll">
    <img src={setup.qr} alt="QR code for the authenticator app" width="176" height="176" />
    <div class="stack">
      <p>Scan the code, or enter this key manually:</p>
      <code class="secret">{setup.secret.replace(/(.{4})/g, '$1 ').trim()}</code>
      <form class="stack" onsubmit={confirm}>
        <label class="field">
          <span>Code from the app</span>
          <input class="input mono" inputmode="numeric" autocomplete="one-time-code" maxlength="7" bind:value={code} required />
        </label>
        <div><button class="btn primary" type="submit" disabled={busy}>Enable</button></div>
      </form>
    </div>
  </div>
{/if}

<style>
  .enroll {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
    align-items: flex-start;
  }
  img {
    background: #fff;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 6px;
  }
  .secret {
    padding: 6px 8px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    word-break: break-all;
  }
</style>
