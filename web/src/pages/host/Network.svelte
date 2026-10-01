<script lang="ts">
  import ProbeNote from '../../components/ProbeNote.svelte';
  import EmptyState from '../../components/EmptyState.svelte';
  import { count, rate } from '../../lib/format';
  import type { HostDetail } from '../../lib/types';

  interface Props {
    detail: HostDetail;
  }
  let { detail }: Props = $props();
  const listening = $derived(detail.medium?.data.listening ?? null);
  const net = $derived(detail.metrics?.data.net ?? []);
  const tcp = $derived(detail.metrics?.data.tcp ?? null);
  let filter = $state('');
  const sockets = $derived(
    listening?.status === 'ok'
      ? listening.data.filter((s) => !filter || `${s.proto} ${s.address} ${s.port} ${s.process ?? ''}`.toLowerCase().includes(filter.toLowerCase()))
      : [],
  );
</script>

<div class="stack">
  <section class="panel">
    <header class="panel-header">
      <h2>Listening ports</h2>
      {#if listening?.status === 'ok'}<span class="muted">via {listening.source}</span>{/if}
      <span class="spacer"></span>
      <input class="input" placeholder="Filter" bind:value={filter} aria-label="Filter listening ports" />
    </header>
    {#if !listening}
      <EmptyState title="Not collected yet" />
    {:else if listening.status === 'na'}
      <ProbeNote reason={listening.reason} />
    {:else if sockets.length === 0}
      <EmptyState title="No listening sockets match" />
    {:else}
      <div class="table-wrap">
        <table class="data">
          <thead><tr><th>Protocol</th><th>Address</th><th class="num">Port</th><th>Process</th><th class="num">PID</th></tr></thead>
          <tbody>
            {#each sockets as s (`${s.proto}|${s.address}|${s.port}`)}
              <tr>
                <td>{s.proto}</td>
                <td class="mono">{s.address}</td>
                <td class="num mono">{s.port}</td>
                <td class="mono">{s.process ?? '–'}</td>
                <td class="num mono">{s.pid ?? '–'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      {#if sockets.some((s) => s.process === null)}
        <p class="muted note">Owning processes are shown only where the SSH user may see them (run as a user in the relevant groups or root to see all).</p>
      {/if}
    {/if}
  </section>

  <section class="panel">
    <header class="panel-header"><h2>Interfaces</h2><span class="muted">current rates</span></header>
    {#if net.length === 0}
      <EmptyState title="No interface rates yet">Rates need two samples.</EmptyState>
    {:else}
      <div class="table-wrap">
        <table class="data">
          <thead><tr><th>Interface</th><th class="num">Received</th><th class="num">Sent</th><th class="num">Packets in</th><th class="num">Packets out</th><th class="num">Errors</th><th class="num">Drops</th></tr></thead>
          <tbody>
            {#each net as n (n.name)}
              <tr>
                <td class="mono">{n.name}</td>
                <td class="num">{rate(n.rx_bps)}</td>
                <td class="num">{rate(n.tx_bps)}</td>
                <td class="num">{count(n.rx_pps)}/s</td>
                <td class="num">{count(n.tx_pps)}/s</td>
                <td class="num">{count(n.rx_errors + n.tx_errors)}/s</td>
                <td class="num">{count(n.rx_drops + n.tx_drops)}/s</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>

  {#if tcp}
    <section class="panel">
      <header class="panel-header"><h2>TCP states</h2></header>
      <div class="panel-body">
        <dl class="props">
          <dt>Established</dt><dd class="num">{count(tcp.established)}</dd>
          <dt>Listening</dt><dd class="num">{count(tcp.listen)}</dd>
          <dt>Time-wait</dt><dd class="num">{count(tcp.time_wait)}</dd>
          <dt>Close-wait</dt><dd class="num">{count(tcp.close_wait)}</dd>
          <dt>Other</dt><dd class="num">{count(tcp.other)}</dd>
          <dt>Total</dt><dd class="num">{count(tcp.total)}</dd>
        </dl>
      </div>
    </section>
  {/if}
</div>

<style>
  .note {
    padding: 8px 12px;
    font-size: 12px;
    border-top: 1px solid var(--border);
  }
  .panel-header .input {
    width: 200px;
    height: 26px;
  }
</style>
