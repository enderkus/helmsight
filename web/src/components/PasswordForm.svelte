<script lang="ts">
  import { api, ApiError } from '../lib/api';

  interface Props {
    ondone: () => void;
  }
  let { ondone }: Props = $props();
  let current = $state('');
  let next = $state('');
  let confirm = $state('');
  let busy = $state(false);
  let error = $state('');

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    error = '';
    if (next !== confirm) {
      error = 'The new passwords do not match.';
      return;
    }
    busy = true;
    try {
      await api.post('/auth/password', { current, new: next });
      current = next = confirm = '';
      ondone();
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Could not change the password.';
    } finally {
      busy = false;
    }
  }
</script>

<form class="stack" onsubmit={submit}>
  {#if error}<p class="banner critical" role="alert">{error}</p>{/if}
  <label class="field"><span>Current password</span><input class="input" type="password" autocomplete="current-password" bind:value={current} required /></label>
  <label class="field">
    <span>New password</span>
    <input class="input" type="password" autocomplete="new-password" minlength="12" bind:value={next} required />
    <span class="hint">At least 12 characters, not containing your user name. Other sessions are signed out.</span>
  </label>
  <label class="field"><span>Repeat new password</span><input class="input" type="password" autocomplete="new-password" bind:value={confirm} required /></label>
  <div><button class="btn primary" type="submit" disabled={busy}>{busy ? 'Saving…' : 'Change password'}</button></div>
</form>
