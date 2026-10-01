<script lang="ts">
  import EmptyState from '../../components/EmptyState.svelte';
  import { bytes, pct } from '../../lib/format';
  import type { HostDetail, Process } from '../../lib/types';

  interface Props {
    detail: HostDetail;
  }
  let { detail }: Props = $props();
  const procs = $derived(detail.metrics?.data.procs ?? null);
</script>

{#snippet table(title: string, list: Process[], note: string)}
  <section class="panel">
    <header class="panel-header"><h2>{title}</h2><span class="muted">{note}</span></header>
    {#if list.length === 0}
      <EmptyState title="No data yet">CPU usage per process needs two samples; check back in a few seconds.</EmptyState>
    {:else}
      <div class="table-wrap">
        <table class="data">
          <thead>
            <tr><th class="num">PID</th><th>User</th><th>Process</th><th class="num">CPU</th><th class="num">Memory</th><th class="num">RSS</th><th class="num">Threads</th><th>State</th></tr>
          </thead>
          <tbody>
            {#each list as p (p.pid)}
              <tr>
                <td class="num mono">{p.pid}</td>
                <td class="mono">{p.user ?? '–'}</td>
                <td class="cmd">
                  <span class="mono name">{p.name}</span>
                  {#if p.command}<span class="mono muted args truncate" title={p.command}>{p.command}</span>{/if}
                </td>
                <td class="num">{p.cpu_pct === null ? '–' : pct(p.cpu_pct)}</td>
                <td class="num">{pct(p.mem_pct)}</td>
                <td class="num" title={`${p.rss_bytes} bytes`}>{bytes(p.rss_bytes)}</td>
                <td class="num">{p.threads}</td>
                <td class="mono">{p.state}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>
{/snippet}

{#if !procs}
  <div class="panel"><EmptyState title="No process data">Processes are read from /proc on every collection.</EmptyState></div>
{:else}
  <div class="stack">
    {@render table('Top by CPU', procs.top_cpu, `${procs.total} processes · % of one core`)}
    {@render table('Top by memory', procs.top_mem, 'resident set size')}
  </div>
{/if}

<style>
  .cmd {
    max-width: 520px;
  }
  .name {
    font-weight: 500;
    margin-right: 8px;
  }
  .args {
    display: inline-block;
    max-width: 380px;
    vertical-align: bottom;
    font-size: 11.5px;
  }
</style>
