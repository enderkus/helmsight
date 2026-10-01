<script lang="ts">
  import TimeSeries from '../components/TimeSeries.svelte';
  import StatusBadge from '../components/StatusBadge.svelte';
  import ErrorState from '../components/ErrorState.svelte';
  import SkeletonRows from '../components/SkeletonRows.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import { api, ApiError, qs } from '../lib/api';
  import { count, datetime } from '../lib/format';
  import type { SecurityOverview } from '../lib/types';

  interface Props {
    params?: Record<string, string>;
  }
  let { params: _params = {} }: Props = $props();
  let days = $state(7);
  let data = $state<SecurityOverview | null>(null);
  let error = $state<string | null>(null);
  let sort = $state<'logins' | 'security' | 'name'>('security');

  async function load() {
    try {
      data = await api.get<SecurityOverview>(`/security${qs({ from: Math.floor(Date.now() / 1000) - days * 86400 })}`);
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Cannot load security overview.';
    }
  }
  $effect(() => {
    void days;
    void load();
  });

  const hosts = $derived(
    [...(data?.hosts ?? [])].sort((x, y) => {
      if (sort === 'name') return x.name.localeCompare(y.name);
      if (sort === 'logins') return (y.failed_logins_24h ?? -1) - (x.failed_logins_24h ?? -1);
      return (y.security_updates ?? -1) - (x.security_updates ?? -1) || (y.pending_updates ?? -1) - (x.pending_updates ?? -1);
    }),
  );
  const stats = $derived({
    logins: data?.auth.total ?? 0,
    withSecurity: (data?.hosts ?? []).filter((h) => (h.security_updates ?? 0) > 0).length,
    reboot: (data?.hosts ?? []).filter((h) => h.reboot_required).length,
    newPorts: (data?.hosts ?? []).filter((h) => h.new_ports.length > 0).length,
    certsSoon: (data?.certs ?? []).filter((c) => c.error || (c.days_left !== null && c.days_left < 21)).length,
  });

  function certState(days: number | null, err: string | null): { label: string; tone: string } {
    if (err) return { label: 'Check failed', tone: 'critical' };
    if (days === null) return { label: 'Unknown', tone: 'unknown' };
    if (days < 0) return { label: 'Expired', tone: 'critical' };
    if (days < 7) return { label: 'Expires soon', tone: 'critical' };
    if (days < 21) return { label: 'Renew soon', tone: 'warning' };
    return { label: 'Valid', tone: 'ok' };
  }
</script>

<div class="page">
  <div class="page-header">
    <h1>Security</h1>
    <span class="spacer"></span>
    <select class="select" bind:value={days} aria-label="Period">
      <option value={1}>Last 24 hours</option><option value={7}>Last 7 days</option><option value={30}>Last 30 days</option>
    </select>
  </div>
  {#if error}<ErrorState message={error} onretry={load} />
  {:else if !data}<div class="panel"><SkeletonRows rows={8} /></div>
  {:else}
    <div class="stack">
      <div class="tiles">
        <div class="tile panel"><span class="label">Failed SSH logins</span><span class="value">{count(stats.logins)}</span><span class="hint">in period, all hosts</span></div>
        <div class="tile panel"><span class="label">Hosts with security updates</span><span class="value">{stats.withSecurity}</span><span class="hint">of {data.hosts.length}</span></div>
        <div class="tile panel"><span class="label">Reboot required</span><span class="value">{stats.reboot}</span><span class="hint">hosts</span></div>
        <div class="tile panel"><span class="label">Unexpected ports</span><span class="value">{stats.newPorts}</span><span class="hint">hosts with ports not on their baseline</span></div>
        <div class="tile panel"><span class="label">Certificates needing attention</span><span class="value">{stats.certsSoon}</span><span class="hint">of {data.certs.length + data.certs_pending.length} checked endpoints</span></div>
      </div>

      <TimeSeries
        title="Failed SSH logins, all hosts"
        xs={data.auth.histogram.map((b) => b.ts)}
        series={[{ label: 'failed logins', slot: 2, values: data.auth.histogram.map((b) => b.count) }]}
        format={(v) => count(v)}
        bars
        empty="No failed logins recorded in this period."
      />

      <section class="panel">
        <header class="panel-header">
          <h2>Hosts</h2>
          <span class="spacer"></span>
          <label class="muted small" for="ssort">Sort</label>
          <select id="ssort" class="select" bind:value={sort}>
            <option value="security">Security updates</option><option value="logins">Failed logins</option><option value="name">Name</option>
          </select>
        </header>
        <div class="table-wrap">
          <table class="data">
            <thead>
              <tr><th>Host</th><th>Status</th><th class="num">Failed logins (24h)</th><th class="num">Pending updates</th><th class="num">Security</th><th>Reboot</th><th>Ports not on baseline</th></tr>
            </thead>
            <tbody>
              {#each hosts as h (h.name)}
                <tr>
                  <td><a class="mono" href={`/hosts/${encodeURIComponent(h.name)}?tab=security`}>{h.name}</a></td>
                  <td><StatusBadge status={h.status} /></td>
                  <td class="num" title={h.auth_unavailable ?? h.auth_source ?? ''}>{h.auth_unavailable ? 'n/a' : count(h.failed_logins_24h)}</td>
                  <td class="num" title={h.updates_unavailable ?? (h.updates_checked_at ? `checked ${datetime(h.updates_checked_at)}` : 'not checked yet')}>
                    {h.updates_unavailable ? 'n/a' : count(h.pending_updates)}
                  </td>
                  <td class="num">{h.updates_unavailable ? 'n/a' : h.security_updates === null ? '–' : count(h.security_updates)}</td>
                  <td title={h.reboot_reasons.join('; ')}>{h.reboot_required === null ? 'unknown' : h.reboot_required ? 'required' : 'no'}</td>
                  <td class="mono ports">{h.baseline ? (h.new_ports.length ? h.new_ports.join(', ') : 'none') : 'no baseline'}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </section>

      <section class="panel">
        <header class="panel-header"><h2>TLS certificates</h2><span class="muted">checked from this server</span></header>
        {#if data.certs.length === 0 && data.certs_pending.length === 0}
          <EmptyState title="No endpoints configured">Add <code>[[certs]] endpoint = "host:443"</code> to the configuration.</EmptyState>
        {:else}
          <div class="table-wrap">
            <table class="data">
              <thead><tr><th>Endpoint</th><th>State</th><th class="num">Days left</th><th>Expires</th><th>Subject</th><th>Issuer</th><th>Checked</th></tr></thead>
              <tbody>
                {#each data.certs as c (c.endpoint)}
                  {@const st = certState(c.days_left, c.error)}
                  <tr>
                    <td class="mono">{c.endpoint}</td>
                    <td><span class={`dot-label tone-${st.tone}`}>{st.label}</span>{#if c.trust_error}<span class="muted small" title={c.trust_error}> · not trusted</span>{/if}</td>
                    <td class="num">{c.days_left === null ? '–' : Math.floor(c.days_left)}</td>
                    <td class="nowrap">{datetime(c.not_after)}</td>
                    <td class="truncate cell" title={c.subject ?? c.error ?? ''}>{c.error ?? c.subject ?? '–'}</td>
                    <td class="truncate cell muted" title={c.issuer ?? ''}>{c.issuer ?? '–'}</td>
                    <td class="nowrap muted">{datetime(c.checked_at)}</td>
                  </tr>
                {/each}
                {#each data.certs_pending as e (e)}
                  <tr><td class="mono">{e}</td><td class="muted">Not checked yet</td><td></td><td></td><td></td><td></td><td></td></tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 8px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 12px;
  }
  .tile .label {
    font-size: 12px;
    color: var(--text-2);
  }
  .tile .value {
    font-size: 20px;
    font-weight: 600;
  }
  .tile .hint {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .small {
    font-size: 12px;
  }
  .ports {
    max-width: 320px;
    white-space: normal;
    font-size: 11.5px;
  }
  .cell {
    max-width: 240px;
  }
  .dot-label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .dot-label::before {
    content: '';
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--unknown);
  }
  .tone-ok::before {
    background: var(--ok);
  }
  .tone-warning::before {
    background: var(--warning);
  }
  .tone-critical::before {
    background: var(--critical);
  }
</style>
