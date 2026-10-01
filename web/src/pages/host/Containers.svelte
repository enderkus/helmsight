<script lang="ts">
  import ProbeNote from '../../components/ProbeNote.svelte';
  import EmptyState from '../../components/EmptyState.svelte';
  import { bytes, pct } from '../../lib/format';
  import type { HostDetail } from '../../lib/types';

  interface Props {
    detail: HostDetail;
  }
  let { detail }: Props = $props();
  const probe = $derived(detail.medium?.data.containers ?? null);
</script>

<section class="panel">
  <header class="panel-header">
    <h2>Containers</h2>
    {#if probe?.status === 'ok'}<span class="muted">via {probe.source}</span>{/if}
  </header>
  {#if !probe}
    <EmptyState title="Not collected yet" />
  {:else if probe.status === 'na'}
    <ProbeNote reason={probe.reason} />
  {:else if probe.data.length === 0}
    <EmptyState title="No containers on this host" />
  {:else}
    <div class="table-wrap">
      <table class="data">
        <thead><tr><th>Name</th><th>Image</th><th>State</th><th>Status</th><th class="num">CPU</th><th class="num">Memory</th><th class="mono">ID</th></tr></thead>
        <tbody>
          {#each probe.data as c (c.id || c.name)}
            <tr>
              <td class="mono">{c.name}</td>
              <td class="mono muted">{c.image}</td>
              <td><span class={`state s-${c.state === 'running' ? 'ok' : c.state === 'restarting' || c.state === 'dead' ? 'bad' : 'idle'}`}>{c.state}</span></td>
              <td class="secondary">{c.status}</td>
              <td class="num">{c.cpu_pct === null ? '–' : pct(c.cpu_pct)}</td>
              <td class="num" title={c.mem_limit ? `limit ${bytes(c.mem_limit)}` : undefined}>{c.mem_bytes === null ? '–' : bytes(c.mem_bytes)}</td>
              <td class="mono muted">{c.id}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

<style>
  .state {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .state::before {
    content: '';
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--unknown);
  }
  .s-ok::before {
    background: var(--ok);
  }
  .s-bad::before {
    background: var(--critical);
  }
</style>
