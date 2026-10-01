<script lang="ts">
  import { OctagonAlert } from '@lucide/svelte';
  import ProbeNote from '../../components/ProbeNote.svelte';
  import EmptyState from '../../components/EmptyState.svelte';
  import type { HostDetail, Service } from '../../lib/types';

  interface Props {
    detail: HostDetail;
  }
  let { detail }: Props = $props();
  const probe = $derived(detail.medium?.data.services ?? null);
  let filter = $state('');
  let showAll = $state(false);

  const isFailed = (s: Service) => s.active === 'failed' || s.sub === 'failed' || s.sub === 'crashed';
  const all = $derived(probe?.status === 'ok' ? probe.data : []);
  const failed = $derived(all.filter(isFailed));
  const list = $derived(
    all
      .filter((s) => !isFailed(s))
      .filter((s) => showAll || s.active === 'active' || s.active === 'activating' || s.active === 'reloading')
      .filter((s) => !filter || `${s.unit} ${s.description}`.toLowerCase().includes(filter.toLowerCase())),
  );
</script>

{#if !probe}
  <div class="panel"><EmptyState title="Not collected yet" /></div>
{:else if probe.status === 'na'}
  <div class="panel"><ProbeNote reason={probe.reason} /></div>
{:else}
  <div class="stack">
    {#if failed.length}
      <section class="panel failed">
        <header class="panel-header">
          <OctagonAlert size={15} aria-hidden="true" />
          <h2>{failed.length} failed {failed.length === 1 ? 'service' : 'services'}</h2>
        </header>
        <div class="table-wrap">
          <table class="data">
            <thead><tr><th>Unit</th><th>State</th><th>Description</th></tr></thead>
            <tbody>
              {#each failed as s (s.unit)}
                <tr><td class="mono">{s.unit}</td><td>{s.active} ({s.sub})</td><td class="secondary">{s.description}</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
      </section>
    {/if}
    <section class="panel">
      <header class="panel-header">
        <h2>{showAll ? 'All services' : 'Running services'}</h2>
        <span class="muted">via {probe.source}</span>
        <span class="spacer"></span>
        <label class="row small"><input type="checkbox" bind:checked={showAll} /> Show inactive</label>
        <input class="input" placeholder="Filter" bind:value={filter} aria-label="Filter services" />
      </header>
      {#if list.length === 0}
        <EmptyState title="No services match" />
      {:else}
        <div class="table-wrap">
          <table class="data">
            <thead><tr><th>Unit</th><th>Load</th><th>Active</th><th>Sub-state</th><th>Description</th></tr></thead>
            <tbody>
              {#each list as s (s.unit)}
                <tr>
                  <td class="mono">{s.unit}</td>
                  <td class="muted">{s.load}</td>
                  <td>{s.active}</td>
                  <td class="muted">{s.sub}</td>
                  <td class="secondary">{s.description}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>
  </div>
{/if}

<style>
  .failed {
    border-left: 3px solid var(--critical);
  }
  .failed :global(.panel-header svg) {
    color: var(--critical);
  }
  .small {
    font-size: 12px;
    color: var(--text-2);
  }
  .panel-header .input {
    width: 200px;
    height: 26px;
  }
</style>
