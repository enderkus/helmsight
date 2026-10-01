<script lang="ts">
  import { ArrowLeftRight } from '@lucide/svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import ErrorState from '../components/ErrorState.svelte';
  import SkeletonRows from '../components/SkeletonRows.svelte';
  import { api, ApiError, qs } from '../lib/api';
  import { fleet } from '../lib/fleet.svelte';
  import { router } from '../lib/router.svelte';
  import { datetime } from '../lib/format';
  import type { Comparison } from '../lib/types';

  interface Props {
    params?: Record<string, string>;
  }
  let { params: _params = {} }: Props = $props();

  const names = $derived(Object.keys(fleet.hosts).sort());
  let a = $state(router.query.get('a') ?? '');
  let b = $state(router.query.get('b') ?? '');
  let kind = $state('');
  let search = $state('');
  let result = $state<Comparison | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);

  const baselineOfA = $derived(fleet.hosts[a]?.baseline ?? null);

  async function run() {
    if (!a) return;
    loading = true;
    try {
      result = await api.get<Comparison>(`/compare${qs({ a, b: b || null })}`);
      error = null;
    } catch (e) {
      result = null;
      error = e instanceof ApiError ? e.message : 'Comparison failed.';
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    router.setQuery({ a: a || null, b: b || null });
    if (a && (b || baselineOfA)) void run();
    else result = null;
  });

  const shown = $derived(
    (result?.differences ?? []).filter(
      (d) => (!kind || d.kind === kind) && (!search || d.subject.toLowerCase().includes(search.toLowerCase())),
    ),
  );
  const kinds = $derived(Object.entries(result?.counts ?? {}));
  const labels: Record<string, string> = { package: 'Packages', port: 'Ports', unit: 'Units', kernel: 'Kernel', os: 'OS' };
</script>

<div class="page">
  <div class="page-header"><h1>Compare</h1><span class="muted">Only differences are shown.</span></div>

  <div class="pick panel">
    <label class="field">
      <span>Host</span>
      <select class="select" bind:value={a}>
        <option value="">Choose a host…</option>
        {#each names as n (n)}<option value={n}>{n}</option>{/each}
      </select>
    </label>
    <button class="btn icon" type="button" title="Swap" aria-label="Swap hosts" disabled={!a || !b} onclick={() => ([a, b] = [b, a])}><ArrowLeftRight size={15} /></button>
    <label class="field">
      <span>Reference</span>
      <select class="select" bind:value={b}>
        <option value="">{baselineOfA ? `Baseline (${baselineOfA})` : 'Choose a host…'}</option>
        {#each names.filter((n) => n !== a) as n (n)}<option value={n}>{n}</option>{/each}
      </select>
    </label>
  </div>

  {#if !a}
    <div class="panel"><EmptyState title="Choose a host to compare">Pick a reference host, or configure a <code>baseline</code> for the host.</EmptyState></div>
  {:else if !b && !baselineOfA}
    <div class="panel"><EmptyState title="Choose a reference host">This host has no baseline configured.</EmptyState></div>
  {:else if error}
    <ErrorState message={error} onretry={run} />
  {:else if !result || loading}
    <div class="panel"><SkeletonRows /></div>
  {:else}
    <div class="summary">
      <p>
        <span class="mono strong">{result.a}</span> <span class="muted">(inventory {datetime(result.a_ts)})</span> compared with
        <span class="mono strong">{result.b}</span> <span class="muted">(inventory {datetime(result.b_ts)})</span>
      </p>
      {#if result.incomplete.length}
        <p class="banner warning">Not compared (unknown on one side): {result.incomplete.join(', ')}.</p>
      {/if}
    </div>
    {#if result.differences.length === 0}
      <div class="panel"><EmptyState title="No differences">Packages, listening ports, enabled units, kernel and OS match.</EmptyState></div>
    {:else}
      <section class="panel">
        <header class="panel-header">
          <div class="segmented" role="group" aria-label="Kind">
            <button type="button" aria-pressed={kind === ''} onclick={() => (kind = '')}>All <span class="num">{result.differences.length}</span></button>
            {#each kinds as [k, n] (k)}
              <button type="button" aria-pressed={kind === k} onclick={() => (kind = k)}>{labels[k] ?? k} <span class="num">{n}</span></button>
            {/each}
          </div>
          <span class="spacer"></span>
          <input class="input" placeholder="Filter by name" bind:value={search} aria-label="Filter differences" />
        </header>
        <div class="table-wrap limited">
          <table class="data">
            <thead>
              <tr><th>Kind</th><th>Name</th><th class="mono">{result.a}</th><th class="mono">{result.b}</th></tr>
            </thead>
            <tbody>
              {#each shown as d (d.kind + d.subject)}
                <tr>
                  <td class="muted">{labels[d.kind] ?? d.kind}</td>
                  <td class="mono strong">{d.subject}</td>
                  <td class="mono" class:absent={!d.new}>{d.new ?? 'not present'}</td>
                  <td class="mono" class:absent={!d.old}>{d.old ?? 'not present'}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </section>
    {/if}
  {/if}
</div>

<style>
  .pick {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    padding: 12px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
  .pick .select {
    min-width: 220px;
  }
  .summary {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 12px;
  }
  .strong {
    font-weight: 600;
  }
  .absent {
    color: var(--text-3);
    font-style: italic;
    font-family: var(--font-sans);
  }
  .limited {
    max-height: calc(100vh - 300px);
  }
  .panel-header .input {
    width: 220px;
    height: 26px;
  }
</style>
