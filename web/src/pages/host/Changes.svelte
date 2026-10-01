<script lang="ts">
  import ChangeTimeline from '../../components/ChangeTimeline.svelte';
  import EmptyState from '../../components/EmptyState.svelte';
  import ErrorState from '../../components/ErrorState.svelte';
  import SkeletonRows from '../../components/SkeletonRows.svelte';
  import { api, ApiError, qs, seg } from '../../lib/api';
  import { fleet } from '../../lib/fleet.svelte';
  import type { Change } from '../../lib/types';

  interface Props {
    host: string;
  }
  let { host }: Props = $props();
  let days = $state(7);
  let kind = $state('');
  let list = $state<Change[] | null>(null);
  let error = $state<string | null>(null);

  async function load() {
    try {
      list = await api.get<Change[]>(`/hosts/${seg(host)}/changes${qs({ since: Math.floor(Date.now() / 1000) - days * 86400 })}`);
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

  const shown = $derived((list ?? []).filter((c) => !kind || c.kind === kind));
</script>

<section class="panel">
  <header class="panel-header">
    <h2>What changed</h2>
    <span class="spacer"></span>
    <select class="select" bind:value={kind} aria-label="Kind">
      <option value="">All kinds</option><option value="package">Packages</option><option value="port">Ports</option>
      <option value="unit">Units</option><option value="kernel">Kernel</option><option value="os">OS</option>
    </select>
    <select class="select" bind:value={days} aria-label="Period">
      <option value={1}>Since yesterday</option><option value={7}>Last 7 days</option><option value={30}>Last 30 days</option><option value={90}>Last 90 days</option>
    </select>
  </header>
  {#if error}<div class="panel-body"><ErrorState message={error} onretry={load} /></div>
  {:else if list === null}<SkeletonRows />
  {:else if shown.length === 0}
    <EmptyState title="No changes in this period">Inventory is compared every collection of packages, ports, units, kernel and OS.</EmptyState>
  {:else}
    <ChangeTimeline changes={shown} />
  {/if}
</section>
