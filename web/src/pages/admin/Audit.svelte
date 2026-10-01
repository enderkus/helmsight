<script lang="ts">
  import { ShieldCheck } from '@lucide/svelte';
  import ErrorState from '../../components/ErrorState.svelte';
  import EmptyState from '../../components/EmptyState.svelte';
  import SkeletonRows from '../../components/SkeletonRows.svelte';
  import { api, ApiError, qs } from '../../lib/api';
  import { datetime } from '../../lib/format';
  import type { AuditEntry } from '../../lib/types';

  interface Props {
    params?: Record<string, string>;
  }
  let { params: _params = {} }: Props = $props();

  let entries = $state<AuditEntry[] | null>(null);
  let error = $state<string | null>(null);
  let prefix = $state('');
  let more = $state(false);
  let chain = $state<{ entries: number; broken_at: number | null } | null>(null);
  let expanded = $state<number | null>(null);
  const PAGE = 100;

  async function load(before?: number) {
    try {
      const page = await api.get<AuditEntry[]>(`/audit${qs({ before, action: prefix, limit: PAGE })}`);
      entries = before ? [...(entries ?? []), ...page] : page;
      more = page.length === PAGE;
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Cannot load the audit log.';
    }
  }
  $effect(() => {
    void prefix;
    entries = null;
    void load();
  });

  async function verify() {
    try {
      chain = await api.get('/audit/verify');
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Verification failed.';
    }
  }

  function summary(e: AuditEntry): string {
    const d = e.detail;
    if (e.action === 'action.run') return `${String(d.action)} → exit ${d.exit_status ?? 'n/a'}${d.error ? ` (${String(d.error)})` : ''}`;
    if (e.action === 'action.start') return String(d.command ?? '');
    return Object.entries(d)
      .filter(([, v]) => v !== null && v !== undefined && v !== '')
      .map(([k, v]) => `${k}=${typeof v === 'object' ? JSON.stringify(v) : String(v)}`)
      .join(' · ');
  }
</script>

<div class="page">
  <div class="page-header">
    <h1>Audit log</h1>
    <span class="muted">Append-only and hash-chained.</span>
    <span class="spacer"></span>
    <select class="select" bind:value={prefix} aria-label="Filter">
      <option value="">All events</option><option value="action.">Actions</option><option value="user.">Users</option>
      <option value="hostkey.">Host keys</option><option value="alert.">Alerts</option><option value="silence.">Silences</option>
      <option value="display_token.">Display tokens</option><option value="secret.">Secrets</option>
    </select>
    <button class="btn" type="button" onclick={verify}><ShieldCheck size={14} />Verify integrity</button>
  </div>
  {#if chain}
    <div class={`banner ${chain.broken_at ? 'critical' : 'info'}`} role="status">
      <div class="body">
        {#if chain.broken_at}
          The hash chain is broken at entry #{chain.broken_at}: the database was modified outside the application.
        {:else}
          All {chain.entries} entries verified; the chain is intact.
        {/if}
      </div>
    </div>
  {/if}
  {#if error}<ErrorState message={error} onretry={() => load()} />
  {:else if !entries}<div class="panel"><SkeletonRows rows={8} /></div>
  {:else if entries.length === 0}<div class="panel"><EmptyState title="No entries" /></div>
  {:else}
    <div class="panel table-wrap">
      <table class="data">
        <thead><tr><th class="num">#</th><th>Time</th><th>Actor</th><th>Event</th><th>Target</th><th>Details</th></tr></thead>
        <tbody>
          {#each entries as e (e.id)}
            <tr class="entry" onclick={() => (expanded = expanded === e.id ? null : e.id)}>
              <td class="num mono muted">{e.id}</td>
              <td class="nowrap mono">{datetime(e.ts)}</td>
              <td class="mono">{e.actor}</td>
              <td class="mono">{e.action}</td>
              <td class="mono">{e.target ?? '–'}</td>
              <td class="detail truncate">
                <button class="linklike" type="button" aria-expanded={expanded === e.id} onclick={(ev) => { ev.stopPropagation(); expanded = expanded === e.id ? null : e.id; }}>{summary(e) || '–'}</button>
              </td>
            </tr>
            {#if expanded === e.id}
              <tr class="expanded">
                <td></td>
                <td colspan="5">
                  <pre class="output">{JSON.stringify(e.detail, null, 2)}</pre>
                  <p class="muted mono hash">hash {e.hash}</p>
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>
    {#if more}
      <div class="more"><button class="btn" type="button" onclick={() => load(entries?.[entries.length - 1]?.id)}>Load older entries</button></div>
    {/if}
  {/if}
</div>

<style>
  .banner {
    margin-bottom: 12px;
  }
  .detail {
    max-width: 480px;
  }
  .linklike {
    all: unset;
    cursor: pointer;
    color: var(--text-2);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: block;
  }
  .linklike:focus-visible {
    outline: 2px solid var(--focus);
  }
  .entry {
    cursor: pointer;
  }
  .expanded td {
    padding-top: 6px;
    padding-bottom: 10px;
    height: auto;
  }
  .hash {
    margin-top: 6px;
    font-size: 11px;
  }
  .more {
    margin-top: 12px;
    display: flex;
    justify-content: center;
  }
</style>
