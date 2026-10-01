<script lang="ts">
  import ChangeTimeline from '../components/ChangeTimeline.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import ErrorState from '../components/ErrorState.svelte';
  import SkeletonRows from '../components/SkeletonRows.svelte';
  import { api, ApiError, qs } from '../lib/api';
  import { fleet } from '../lib/fleet.svelte';
  import type { Change } from '../lib/types';

  interface Props {
    params?: Record<string, string>;
  }
  let { params: _params = {} }: Props = $props();
  let days = $state(1);
  let kind = $state('');
  let host = $state('');
  let list = $state<Change[] | null>(null);
  let error = $state<string | null>(null);

  async function load() {
    try {
      list = await api.get<Change[]>(`/changes${qs({ since: Math.floor(Date.now() / 1000) - days * 86400, limit: 5000 })}`);
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Cannot load changes.';
    }
  }
  $effect(() => {
    void days;
    void fleet.changesVersion;
    void load();
  });
  const hosts = $derived([...new Set((list ?? []).map((c) => c.host))].sort());
  const shown = $derived((list ?? []).filter((c) => (!kind || c.kind === kind) && (!host || c.host === host)));
</script>

<div class="page">
  <div class="page-header">
    <h1>Changes</h1>
    <span class="muted">Packages, listening ports, enabled units, kernel and OS across the fleet.</span>
  </div>
  <div class="filters row">
    <select class="select" bind:value={days} aria-label="Period">
      <option value={1}>Since yesterday</option><option value={7}>Last 7 days</option><option value={30}>Last 30 days</option>
    </select>
    <select class="select" bind:value={kind} aria-label="Kind">
      <option value="">All kinds</option><option value="package">Packages</option><option value="port">Ports</option>
      <option value="unit">Units</option><option value="kernel">Kernel</option><option value="os">OS</option>
    </select>
    <select class="select" bind:value={host} aria-label="Host">
      <option value="">All hosts</option>
      {#each hosts as h (h)}<option value={h}>{h}</option>{/each}
    </select>
    <span class="muted">{shown.length} changes</span>
  </div>
  <section class="panel">
    {#if error}<div class="panel-body"><ErrorState message={error} onretry={load} /></div>
    {:else if list === null}<SkeletonRows rows={8} />
    {:else if shown.length === 0}
      <EmptyState title="No changes in this period" />
    {:else}
      <ChangeTimeline changes={shown} showHost />
    {/if}
  </section>
</div>

<style>
  .filters {
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
</style>
