<script lang="ts">
  import TimeSeries from '../../components/TimeSeries.svelte';
  import ProbeNote from '../../components/ProbeNote.svelte';
  import EmptyState from '../../components/EmptyState.svelte';
  import ErrorState from '../../components/ErrorState.svelte';
  import SkeletonRows from '../../components/SkeletonRows.svelte';
  import { api, ApiError, qs, seg } from '../../lib/api';
  import { count, datetime } from '../../lib/format';
  import type { HostSecurity, TopSource } from '../../lib/types';

  interface Props {
    host: string;
  }
  let { host }: Props = $props();
  let days = $state(7);
  let data = $state<HostSecurity | null>(null);
  let error = $state<string | null>(null);

  async function load() {
    try {
      data = await api.get<HostSecurity>(`/hosts/${seg(host)}/security${qs({ from: Math.floor(Date.now() / 1000) - days * 86400 })}`);
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Cannot load security data.';
    }
  }
  $effect(() => {
    void days;
    void load();
  });
  const updates = $derived(data?.updates?.data ?? null);
</script>

{#snippet top(title: string, rows: TopSource[])}
  <section class="panel">
    <header class="panel-header"><h2>{title}</h2></header>
    {#if rows.length === 0}
      <p class="panel-body muted">None.</p>
    {:else}
      <table class="data">
        <tbody>
          {#each rows as r (r.value)}
            <tr><td class="mono">{r.value}</td><td class="num">{count(r.count)}</td></tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>
{/snippet}

{#if error}<ErrorState message={error} onretry={load} />
{:else if !data}<div class="panel"><SkeletonRows /></div>
{:else}
  <div class="stack">
    <div class="range">
      <select class="select" bind:value={days} aria-label="Period">
        <option value={1}>Last 24 hours</option><option value={7}>Last 7 days</option><option value={30}>Last 30 days</option>
      </select>
    </div>
    {#if data.auth.unavailable}
      <section class="panel"><header class="panel-header"><h2>Failed SSH logins</h2></header><ProbeNote reason={data.auth.unavailable} /></section>
    {:else}
      <TimeSeries
        title="Failed SSH logins"
        subtitle={`${count(data.auth.total)} in period${data.auth.source ? ` · from ${data.auth.source}` : ''}`}
        xs={data.auth.histogram.map((b) => b.ts)}
        series={[{ label: 'failed logins', slot: 2, values: data.auth.histogram.map((b) => b.count) }]}
        format={(v) => count(v)}
        bars
        empty="No failed logins in this period."
      />
    {/if}
    <div class="grid-2">
      {@render top('Top source addresses', data.auth.top_ips)}
      {@render top('Top attempted users', data.auth.top_users)}
    </div>

    <section class="panel">
      <header class="panel-header">
        <h2>Pending updates</h2>
        {#if updates?.status === 'ok'}
          <span class="muted">{updates.data.total} total · {updates.data.security === null ? 'security classification not available' : `${updates.data.security} security`} · checked {datetime(data.updates?.ts)}</span>
        {/if}
      </header>
      {#if !updates}
        <EmptyState title="Not checked yet">Pending updates are checked every few hours because it is expensive.</EmptyState>
      {:else if updates.status === 'na'}
        <ProbeNote reason={updates.reason} />
      {:else if updates.data.packages.length === 0}
        <EmptyState title="Up to date">No pending updates were reported.</EmptyState>
      {:else}
        {#each updates.data.notes as n (n)}<p class="note muted">{n}</p>{/each}
        <div class="table-wrap limited">
          <table class="data">
            <thead><tr><th>Package</th><th>Installed</th><th>Available</th><th>Security</th><th>Repository</th></tr></thead>
            <tbody>
              {#each [...updates.data.packages].sort((a, b) => Number(b.security) - Number(a.security)) as p (p.name + p.available)}
                <tr>
                  <td class="mono">{p.name}</td>
                  <td class="mono muted">{p.current ?? '–'}</td>
                  <td class="mono">{p.available}</td>
                  <td>{#if p.security}<span class="sec">security</span>{:else}<span class="muted">–</span>{/if}</td>
                  <td class="muted">{p.repo ?? '–'}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>

    <section class="panel">
      <header class="panel-header">
        <h2>Ports not on baseline</h2>
        {#if data.baseline}<span class="muted">compared with <a class="mono" href={`/hosts/${encodeURIComponent(data.baseline)}`}>{data.baseline}</a></span>{/if}
      </header>
      {#if !data.baseline}
        <p class="panel-body muted">No baseline host is configured for this host. Set <code>baseline = "…"</code> in its host entry.</p>
      {:else if data.new_ports.length === 0}
        <p class="panel-body muted">Every listening port is also present on the baseline.</p>
      {:else}
        <table class="data">
          <thead><tr><th>Port</th><th>Difference</th><th>Listening on</th></tr></thead>
          <tbody>
            {#each data.new_ports as p (p.subject)}
              <tr><td class="mono">{p.subject}</td><td>{p.action === 'added' ? 'only on this host' : 'different listener'}</td><td class="mono">{p.new ?? '–'}</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </section>
  </div>
{/if}

<style>
  .range {
    display: flex;
  }
  .note {
    padding: 8px 12px 0;
    font-size: 12px;
  }
  .limited {
    max-height: 480px;
  }
  .sec {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-weight: 500;
  }
  .sec::before {
    content: '';
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--critical);
  }
</style>
