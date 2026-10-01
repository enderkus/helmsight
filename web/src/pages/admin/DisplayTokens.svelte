<script lang="ts">
  import { Plus, Copy, ExternalLink } from '@lucide/svelte';
  import Dialog from '../../components/Dialog.svelte';
  import EmptyState from '../../components/EmptyState.svelte';
  import ErrorState from '../../components/ErrorState.svelte';
  import SkeletonRows from '../../components/SkeletonRows.svelte';
  import { api, ApiError } from '../../lib/api';
  import { fleet } from '../../lib/fleet.svelte';
  import { toast } from '../../lib/toast.svelte';
  import { clock } from '../../lib/clock.svelte';
  import { ago, datetime } from '../../lib/format';
  import type { DisplayToken } from '../../lib/types';

  interface Props {
    params?: Record<string, string>;
  }
  let { params: _params = {} }: Props = $props();

  let tokens = $state<DisplayToken[] | null>(null);
  let error = $state<string | null>(null);
  let createOpen = $state(false);
  let name = $state('');
  let all = $state(false);
  let picked = $state<string[]>([]);
  let created = $state<{ url: string; name: string } | null>(null);
  let busy = $state(false);

  const groups = $derived([...new Set(Object.values(fleet.hosts).flatMap((h) => h.groups))].sort());

  async function load() {
    try {
      tokens = await api.get<DisplayToken[]>('/display-tokens');
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Cannot load tokens.';
    }
  }
  $effect(() => {
    void load();
  });

  async function create(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      const r = await api.post<{ token: DisplayToken; secret: string }>('/display-tokens', {
        name,
        groups: all ? ['*'] : picked,
      });
      created = { url: `${location.origin}/display#token=${r.secret}`, name: r.token.name };
      createOpen = false;
      name = '';
      picked = [];
      all = false;
      await load();
    } catch (err) {
      toast(err instanceof ApiError ? err.message : 'Could not create the token.', 'error');
    } finally {
      busy = false;
    }
  }

  async function revoke(t: DisplayToken) {
    try {
      await api.del(`/display-tokens/${t.id}`);
      toast(`Token "${t.name}" revoked. Screens using it stop updating.`);
      await load();
    } catch (err) {
      toast(err instanceof ApiError ? err.message : 'Could not revoke.', 'error');
    }
  }

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast('Copied.');
    } catch {
      toast('Copy failed; select the text manually.', 'error');
    }
  }
</script>

<div class="page">
  <div class="page-header">
    <h1>Wall display</h1>
    <a class="btn" href="/display" target="_blank" rel="noopener"><ExternalLink size={14} />Open display</a>
    <span class="spacer"></span>
    <button class="btn primary" type="button" onclick={() => (createOpen = true)}><Plus size={14} />New token</button>
  </div>
  <p class="secondary intro">
    Display tokens let a screen show a read-only status wall for chosen groups without signing in. They grant no other access and can be revoked at any time.
  </p>
  {#if created}
    <div class="banner info">
      <div class="body stack">
        <p class="title">Token "{created.name}" created. Copy the address now; it is not shown again.</p>
        <div class="row"><code class="url">{created.url}</code><button class="btn small" type="button" onclick={() => created && copy(created.url)}><Copy size={13} />Copy</button></div>
      </div>
      <button class="btn ghost small" type="button" onclick={() => (created = null)}>Done</button>
    </div>
  {/if}
  {#if error}<ErrorState message={error} onretry={load} />
  {:else if !tokens}<div class="panel"><SkeletonRows /></div>
  {:else if tokens.length === 0}
    <div class="panel"><EmptyState title="No display tokens" /></div>
  {:else}
    <div class="panel table-wrap">
      <table class="data">
        <thead><tr><th>Name</th><th>Groups</th><th>Created</th><th>Last used</th><th>State</th><th class="num"></th></tr></thead>
        <tbody>
          {#each tokens as t (t.id)}
            <tr class:revoked={!!t.revoked_at}>
              <td>{t.name}</td>
              <td>{#each t.groups as g (g)}<span class="tag">{g === '*' ? 'all hosts' : g}</span> {/each}</td>
              <td title={datetime(t.created_at)}>{t.created_by}, {ago(t.created_at, clock.now)}</td>
              <td>{t.last_used_at ? ago(t.last_used_at, clock.now) : 'never'}</td>
              <td>{t.revoked_at ? `Revoked ${ago(t.revoked_at, clock.now)}` : 'Active'}</td>
              <td class="num">{#if !t.revoked_at}<button class="btn small danger" type="button" onclick={() => revoke(t)}>Revoke</button>{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<Dialog open={createOpen} title="New display token" onclose={() => (createOpen = false)}>
  <form id="new-token" class="stack" onsubmit={create}>
    <label class="field"><span>Name</span><input class="input" bind:value={name} required maxlength="64" placeholder="e.g. NOC screen, 2nd floor" /></label>
    <fieldset class="groups">
      <legend>Visible hosts</legend>
      <label class="row"><input type="checkbox" bind:checked={all} /> All hosts</label>
      {#if !all}
        {#each groups as g (g)}
          <label class="row"><input type="checkbox" value={g} bind:group={picked} /> <span class="tag">{g}</span></label>
        {/each}
        {#if groups.length === 0}<p class="muted">No groups are configured; use “All hosts”.</p>{/if}
      {/if}
    </fieldset>
  </form>
  {#snippet footer()}
    <button class="btn" type="button" onclick={() => (createOpen = false)}>Cancel</button>
    <button class="btn primary" type="submit" form="new-token" disabled={busy || (!all && picked.length === 0)}>Create token</button>
  {/snippet}
</Dialog>

<style>
  .intro {
    margin-bottom: 12px;
    max-width: 760px;
  }
  .banner {
    margin-bottom: 12px;
  }
  .url {
    padding: 4px 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow-wrap: anywhere;
  }
  .groups {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  legend {
    font-size: 12px;
    color: var(--text-2);
    padding: 0 4px;
  }
  .revoked td {
    color: var(--text-3);
  }
</style>
