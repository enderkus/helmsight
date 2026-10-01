<script lang="ts">
  import AuthFrame from '../components/AuthFrame.svelte';
  import PasswordForm from '../components/PasswordForm.svelte';
  import TotpEnroll from '../components/TotpEnroll.svelte';
  import { loadSession, logout } from '../lib/session.svelte';
  import { toast } from '../lib/toast.svelte';

  interface Props {
    step: 'password' | 'totp';
  }
  let { step }: Props = $props();

  async function done() {
    toast(step === 'password' ? 'Password changed.' : 'Two-factor authentication enabled.');
    await loadSession();
  }
</script>

<AuthFrame title={step === 'password' ? 'Change your password' : 'Enable two-factor authentication'}>
  {#if step === 'password'}
    <p class="secondary">An administrator set a temporary password. Choose a new one to continue.</p>
    <PasswordForm ondone={done} />
  {:else}
    <p class="secondary">Administrators must use two-factor authentication on this server.</p>
    <TotpEnroll ondone={done} />
  {/if}
  <button class="btn ghost small" type="button" onclick={() => void logout()}>Sign out</button>
</AuthFrame>
