<script lang="ts">
  import { Play, TriangleAlert } from '@lucide/svelte';
  import Dialog from '../components/Dialog.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import ErrorState from '../components/ErrorState.svelte';
  import SkeletonRows from '../components/SkeletonRows.svelte';
  import { api, ApiError, seg } from '../lib/api';
  import { router } from '../lib/router.svelte';
  import { session } from '../lib/session.svelte';
  import type { ActionView, RunResult } from '../lib/types';

  interface Props {
    params?: Record<string, string>;
  }
  let { params: _params = {} }: Props = $props();

  let actions = $state<ActionView[] | null>(null);
  let error = $state<string | null>(null);
  let chosen = $state<Record<string, string>>({});
  let confirm = $state<{ action: ActionView; host: string } | null>(null);
  let running = $state(false);
  let result = $state<RunResult | null>(null);
  let runError = $state<string | null>(null);
  const preferred = router.query.get('host') ?? '';

  async function load() {
    try {
      actions = await api.get<ActionView[]>('/actions');
      const c: Record<string, string> = {};
      for (const a of actions) c[a.id] = a.hosts.includes(preferred) ? preferred : (a.hosts[0] ?? '');
      chosen = c;
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Cannot load actions.';
    }
  }
  $effect(() => {
    void load();
  });

  async function run() {
    if (!confirm) return;
    running = true;
    runError = null;
    result = null;
    const { action, host } = confirm;
    try {
      result = await api.post<RunResult>(`/actions/${seg(action.id)}/run`, { host });
    } catch (e) {
      runError = e instanceof ApiError ? e.message : 'The action failed.';
    } finally {
      running = false;
      confirm = null;
    }
  }
</script>

<div class="page">
  <div class="page-header">
    <h1>Actions</h1>
    <span class="muted">Pre-approved commands defined by an administrator. Every run is recorded in the audit log.</span>
  </div>

  {#if !session.meta?.features.actions}
    <div class="panel"><EmptyState title="No actions are configured" /></div>
  {:else if error}
    <ErrorState message={error} onretry={load} />
  {:else if !actions}
    <div class="panel"><SkeletonRows /></div>
  {:else if actions.length === 0}
    <div class="panel"><EmptyState title="No actions available to your role" /></div>
  {:else}
    <div class="stack">
      {#if runError}<div class="banner critical" role="alert"><div class="body"><p class="title">Action failed</p><p>{runError}</p></div></div>{/if}
      {#if result}
        <section class="panel result">
          <header class="panel-header">
            <h2>{result.action} on <span class="mono">{result.host}</span></h2>
            <span class="spacer"></span>
            <span class={`exit ${result.exit_status === 0 ? 'ok' : 'bad'}`}>exit {result.exit_status ?? '?'}</span>
            <span class="muted">{result.duration_ms} ms · audit #{result.audit_id}</span>
          </header>
          <div class="panel-body stack">
            <code class="cmd">$ {result.command}</code>
            {#if result.stdout}<pre class="output">{result.stdout}</pre>{/if}
            {#if result.stderr}<p class="muted small">Standard error</p><pre class="output">{result.stderr}</pre>{/if}
            {#if !result.stdout && !result.stderr}<p class="muted">No output.</p>{/if}
            {#if result.truncated}<p class="muted small">Output was truncated.</p>{/if}
          </div>
        </section>
      {/if}
      <div class="list">
        {#each actions as a (a.id)}
          <section class="panel action">
            <header class="panel-header"><h2>{a.label}</h2><span class="spacer"></span><span class="muted small">requires {a.role}</span></header>
            <div class="panel-body stack">
              {#if a.description}<p class="secondary">{a.description}</p>{/if}
              <code class="cmd">{a.command}</code>
              {#if a.hosts.length === 0}
                <p class="muted">No enabled hosts match this action's targets.</p>
              {:else}
                <div class="row">
                  <label class="sr-only" for={`h-${a.id}`}>Host</label>
                  <select id={`h-${a.id}`} class="select" bind:value={chosen[a.id]}>
                    {#each a.hosts as h (h)}<option value={h}>{h}</option>{/each}
                  </select>
                  <button class="btn primary" type="button" disabled={!chosen[a.id] || running} onclick={() => (confirm = { action: a, host: chosen[a.id] ?? '' })}>
                    <Play size={14} />Run…
                  </button>
                </div>
              {/if}
            </div>
          </section>
        {/each}
      </div>
    </div>
  {/if}
</div>

<Dialog open={!!confirm} title="Run action?" onclose={() => (confirm = null)}>
  {#if confirm}
    <div class="banner warning">
      <TriangleAlert size={16} aria-hidden="true" />
      <div class="body">This runs the following command on the host, as the configured SSH user.</div>
    </div>
    <dl class="props">
      <dt>Action</dt><dd>{confirm.action.label}</dd>
      <dt>Host</dt><dd class="mono strong">{confirm.host}</dd>
      <dt>Command</dt><dd><code class="cmd">{confirm.action.command}</code></dd>
      <dt>Timeout</dt><dd>{confirm.action.timeout_secs} s</dd>
    </dl>
  {/if}
  {#snippet footer()}
    <button class="btn" type="button" onclick={() => (confirm = null)}>Cancel</button>
    <button class="btn primary" type="button" disabled={running} onclick={run}>{running ? 'Running…' : 'Run on host'}</button>
  {/snippet}
</Dialog>

<style>
  .list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
    gap: 10px;
  }
  .cmd {
    display: block;
    padding: 6px 8px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow-wrap: anywhere;
  }
  .small {
    font-size: 12px;
  }
  .strong {
    font-weight: 600;
  }
  .exit {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-weight: 600;
    font-size: 12px;
  }
  .exit::before {
    content: '';
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .exit.ok::before {
    background: var(--ok);
  }
  .exit.bad::before {
    background: var(--critical);
  }
</style>
