<script lang="ts">
  import { UserPlus } from '@lucide/svelte';
  import Dialog from '../../components/Dialog.svelte';
  import ErrorState from '../../components/ErrorState.svelte';
  import SkeletonRows from '../../components/SkeletonRows.svelte';
  import { api, ApiError } from '../../lib/api';
  import { session } from '../../lib/session.svelte';
  import { toast } from '../../lib/toast.svelte';
  import { clock } from '../../lib/clock.svelte';
  import { ago } from '../../lib/format';
  import type { Role, User } from '../../lib/types';

  interface Props {
    params?: Record<string, string>;
  }
  let { params: _params = {} }: Props = $props();

  let users = $state<User[] | null>(null);
  let error = $state<string | null>(null);
  let createOpen = $state(false);
  let nName = $state('');
  let nPassword = $state('');
  let nRole = $state<Role>('viewer');
  let editing = $state<User | null>(null);
  let eRole = $state<Role>('viewer');
  let ePassword = $state('');
  let eResetTotp = $state(false);
  let removing = $state<User | null>(null);
  let busy = $state(false);

  async function load() {
    try {
      users = await api.get<User[]>('/users');
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Cannot load users.';
    }
  }
  $effect(() => {
    void load();
  });

  async function create(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      await api.post('/users', { username: nName, password: nPassword, role: nRole });
      toast(`User ${nName} created. They must change the password at first sign-in.`);
      createOpen = false;
      nName = nPassword = '';
      nRole = 'viewer';
      await load();
    } catch (err) {
      toast(err instanceof ApiError ? err.message : 'Could not create the user.', 'error');
    } finally {
      busy = false;
    }
  }

  function edit(u: User) {
    editing = u;
    eRole = u.role;
    ePassword = '';
    eResetTotp = false;
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!editing) return;
    busy = true;
    try {
      await api.patch(`/users/${editing.id}`, {
        role: eRole !== editing.role ? eRole : undefined,
        password: ePassword || undefined,
        reset_totp: eResetTotp || undefined,
      });
      toast('User updated.');
      editing = null;
      await load();
    } catch (err) {
      toast(err instanceof ApiError ? err.message : 'Could not update the user.', 'error');
    } finally {
      busy = false;
    }
  }

  async function setDisabled(u: User, disabled: boolean) {
    try {
      await api.patch(`/users/${u.id}`, { disabled });
      toast(disabled ? `${u.username} disabled and signed out.` : `${u.username} enabled.`);
      await load();
    } catch (err) {
      toast(err instanceof ApiError ? err.message : 'Could not update the user.', 'error');
    }
  }

  async function remove() {
    if (!removing) return;
    busy = true;
    try {
      await api.del(`/users/${removing.id}`);
      toast(`${removing.username} deleted.`);
      removing = null;
      await load();
    } catch (err) {
      toast(err instanceof ApiError ? err.message : 'Could not delete the user.', 'error');
    } finally {
      busy = false;
    }
  }
</script>

<div class="page">
  <div class="page-header">
    <h1>Users</h1>
    <span class="spacer"></span>
    <button class="btn primary" type="button" onclick={() => (createOpen = true)}><UserPlus size={14} />Add user</button>
  </div>
  {#if error}<ErrorState message={error} onretry={load} />
  {:else if !users}<div class="panel"><SkeletonRows /></div>
  {:else}
    <div class="panel table-wrap">
      <table class="data">
        <thead><tr><th>User</th><th>Role</th><th>Sign-in</th><th>Two-factor</th><th>State</th><th>Last sign-in</th><th class="num">Actions</th></tr></thead>
        <tbody>
          {#each users as u (u.id)}
            <tr>
              <td class="mono">{u.username}{#if u.id === session.user?.id}<span class="muted sans"> (you)</span>{/if}</td>
              <td>{u.role}</td>
              <td>{u.oidc_subject ? 'Single sign-on' : 'Password'}</td>
              <td>{u.totp_enabled ? 'Enabled' : '–'}</td>
              <td>{u.disabled ? 'Disabled' : u.must_change_password ? 'Must change password' : 'Active'}</td>
              <td>{u.last_login_at ? ago(u.last_login_at, clock.now) : 'never'}</td>
              <td class="num nowrap">
                <button class="btn small" type="button" onclick={() => edit(u)}>Edit</button>
                {#if u.id !== session.user?.id}
                  <button class="btn small" type="button" onclick={() => setDisabled(u, !u.disabled)}>{u.disabled ? 'Enable' : 'Disable'}</button>
                  <button class="btn small danger" type="button" onclick={() => (removing = u)}>Delete</button>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="muted note">Roles: <strong>viewer</strong> reads everything; <strong>operator</strong> also runs permitted actions, acknowledges alerts and manages silences; <strong>admin</strong> also manages users, host keys, display tokens and reads the audit log.</p>
  {/if}
</div>

<Dialog open={createOpen} title="Add user" onclose={() => (createOpen = false)}>
  <form id="create-user" class="stack" onsubmit={create}>
    <label class="field"><span>User name</span><input class="input" bind:value={nName} required autocomplete="off" /></label>
    <label class="field">
      <span>Temporary password</span>
      <input class="input" type="password" bind:value={nPassword} minlength="12" required autocomplete="new-password" />
      <span class="hint">At least 12 characters. The user must change it at first sign-in.</span>
    </label>
    <label class="field">
      <span>Role</span>
      <select class="select" bind:value={nRole}><option value="viewer">viewer</option><option value="operator">operator</option><option value="admin">admin</option></select>
    </label>
  </form>
  {#snippet footer()}
    <button class="btn" type="button" onclick={() => (createOpen = false)}>Cancel</button>
    <button class="btn primary" type="submit" form="create-user" disabled={busy}>Add user</button>
  {/snippet}
</Dialog>

<Dialog open={!!editing} title={`Edit ${editing?.username ?? ''}`} onclose={() => (editing = null)}>
  <form id="edit-user" class="stack" onsubmit={save}>
    <label class="field">
      <span>Role</span>
      <select class="select" bind:value={eRole} disabled={editing?.id === session.user?.id}>
        <option value="viewer">viewer</option><option value="operator">operator</option><option value="admin">admin</option>
      </select>
    </label>
    {#if !editing?.oidc_subject}
      <label class="field">
        <span>Reset password</span>
        <input class="input" type="password" bind:value={ePassword} minlength="12" autocomplete="new-password" placeholder="Leave empty to keep" />
        <span class="hint">The user must change it at next sign-in; their sessions end.</span>
      </label>
      {#if editing?.totp_enabled}
        <label class="row"><input type="checkbox" bind:checked={eResetTotp} /> Remove two-factor authentication (lost device)</label>
      {/if}
    {/if}
  </form>
  {#snippet footer()}
    <button class="btn" type="button" onclick={() => (editing = null)}>Cancel</button>
    <button class="btn primary" type="submit" form="edit-user" disabled={busy}>Save</button>
  {/snippet}
</Dialog>

<Dialog open={!!removing} title="Delete user?" onclose={() => (removing = null)}>
  <p>Delete <span class="mono">{removing?.username}</span>? Their sessions end immediately. Audit entries are kept.</p>
  {#snippet footer()}
    <button class="btn" type="button" onclick={() => (removing = null)}>Cancel</button>
    <button class="btn danger solid" type="button" disabled={busy} onclick={remove}>Delete</button>
  {/snippet}
</Dialog>

<style>
  .note {
    margin-top: 10px;
    font-size: 12px;
  }
  .sans {
    font-family: var(--font-sans);
  }
</style>
