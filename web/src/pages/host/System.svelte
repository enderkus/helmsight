<script lang="ts">
  import ProbeNote from '../../components/ProbeNote.svelte';
  import { bytes, datetime, duration, count } from '../../lib/format';
  import type { HostDetail } from '../../lib/types';

  interface Props {
    detail: HostDetail;
  }
  let { detail }: Props = $props();
  const inv = $derived(detail.inventory);
  const m = $derived(detail.metrics?.data ?? null);
  const sessions = $derived(detail.medium?.data.sessions ?? []);
  const c = $derived(detail.collection);
</script>

<div class="grid-2">
  <section class="panel">
    <header class="panel-header"><h2>Operating system</h2></header>
    <div class="panel-body">
      <dl class="props">
        <dt>Release</dt><dd>{inv?.os.pretty_name ?? '–'}</dd>
        <dt>Distribution id</dt><dd class="mono">{inv?.os.id ?? '–'}{inv?.os.id_like ? ` (like ${inv.os.id_like})` : ''}</dd>
        <dt>Kernel</dt><dd class="mono">{inv?.kernel.release ?? '–'}</dd>
        <dt>Kernel build</dt><dd class="mono small">{inv?.kernel.version ?? '–'}</dd>
        <dt>Architecture</dt><dd class="mono">{inv?.kernel.machine ?? '–'}</dd>
        <dt>Virtualization</dt><dd>{inv?.virtualization ?? 'unknown'}</dd>
        <dt>Reboot required</dt>
        <dd>
          {inv?.reboot.required === null || inv?.reboot.required === undefined ? 'unknown' : inv.reboot.required ? 'yes' : 'no'}
          {#each inv?.reboot.reasons ?? [] as r (r)}<br /><span class="muted small">{r}</span>{/each}
        </dd>
      </dl>
    </div>
  </section>

  <section class="panel">
    <header class="panel-header"><h2>Hardware and uptime</h2></header>
    <div class="panel-body">
      <dl class="props">
        <dt>CPU model</dt><dd>{inv?.cpu_model ?? '–'}</dd>
        <dt>Cores</dt><dd class="num">{m?.cpu?.cores ?? '–'}</dd>
        <dt>Memory</dt><dd>{m?.mem ? bytes(m.mem.total) : '–'}</dd>
        <dt>Swap</dt><dd>{m?.mem ? (m.mem.swap_total ? bytes(m.mem.swap_total) : 'none') : '–'}</dd>
        <dt>Uptime</dt><dd>{duration(m?.uptime_secs)}</dd>
        <dt>Last boot</dt><dd>{datetime(m?.boot_time)}</dd>
      </dl>
    </div>
  </section>

  <section class="panel">
    <header class="panel-header"><h2>Logged-in users</h2></header>
    {#if sessions.length === 0}
      <p class="panel-body muted">No interactive sessions (or <code>who</code> is unavailable).</p>
    {:else}
      <table class="data">
        <thead><tr><th>User</th><th>Line</th><th>From</th><th>Since</th></tr></thead>
        <tbody>
          {#each sessions as s, i (i)}
            <tr><td class="mono">{s.user}</td><td class="mono">{s.line}</td><td class="mono">{s.from ?? '–'}</td><td>{s.login}</td></tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>

  <section class="panel">
    <header class="panel-header"><h2>Collection</h2></header>
    <div class="panel-body">
      <dl class="props">
        <dt>SSH user</dt><dd class="mono">{detail.basics?.user ?? '–'}{detail.basics?.uid === 0 ? ' (root)' : ''}</dd>
        <dt>Last attempt</dt><dd>{datetime(c.last_attempt)}</dd>
        <dt>Last success</dt><dd>{datetime(c.last_success)}</dd>
        <dt>Duration</dt><dd>{c.duration_ms === null ? '–' : `${c.duration_ms} ms`}</dd>
        <dt>Interval</dt><dd>{c.interval} s</dd>
        <dt>Clock offset</dt><dd>{c.clock_skew === null ? '–' : `${c.clock_skew > 0 ? '+' : ''}${c.clock_skew} s`}</dd>
        <dt>Packages</dt>
        <dd>{inv?.package_count !== null && inv?.package_count !== undefined ? `${count(inv.package_count)} (${inv.package_manager})` : (inv?.packages_unavailable ?? '–')}</dd>
      </dl>
    </div>
  </section>

  <section class="panel wide">
    <header class="panel-header">
      <h2>Enabled units</h2>
      {#if inv?.enabled_units.status === 'ok'}<span class="muted">{inv.enabled_units.data.length} · via {inv.enabled_units.source}</span>{/if}
    </header>
    {#if !inv}
      <p class="panel-body muted">Inventory not collected yet.</p>
    {:else if inv.enabled_units.status === 'na'}
      <ProbeNote reason={inv.enabled_units.reason} />
    {:else}
      <div class="panel-body units">
        {#each inv.enabled_units.data as u (u)}<span class="tag mono">{u}</span>{/each}
      </div>
    {/if}
  </section>
</div>

<style>
  .small {
    font-size: 11.5px;
  }
  .units {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .wide {
    grid-column: 1 / -1;
  }
</style>
