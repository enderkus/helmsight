<script lang="ts">
  import { onMount } from 'svelte';
  import { OctagonAlert, TriangleAlert, CircleCheck, Unplug } from '@lucide/svelte';
  import StatusBadge from '../components/StatusBadge.svelte';
  import { clock } from '../lib/clock.svelte';
  import { statusInfo, statusRank } from '../lib/status';
  import { duration, pct, time } from '../lib/format';
  import { setTheme } from '../lib/theme.svelte';
  import { PRODUCT_NAME } from '../lib/brand';
  import type { DisplayState } from '../lib/types';

  // The token lives in the URL fragment, which browsers never send to the server.
  const hash = new URLSearchParams(location.hash.slice(1));
  const token = hash.get('token');
  const rotateSecs = Number(hash.get('rotate') ?? '20') || 20;

  let wall = $state<DisplayState | null>(null);
  let error = $state<string | null>(null);
  let lastOk = $state(0);
  let page = $state(0);

  async function poll() {
    try {
      const resp = await fetch('/api/v1/display/state', {
        headers: token ? { Authorization: `Bearer ${token}` } : {},
        credentials: token ? 'omit' : 'same-origin',
      });
      if (resp.status === 401) {
        error = token
          ? 'This display token is invalid or was revoked. Ask an administrator for a new address.'
          : 'Sign in or open this page with a display token.';
        return;
      }
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      wall = (await resp.json()) as DisplayState;
      error = null;
      lastOk = Math.floor(Date.now() / 1000);
    } catch {
      error = 'Connection to the server lost. Retrying.';
    }
  }

  onMount(() => {
    const theme = hash.get('theme');
    if (theme === 'dark' || theme === 'light') setTheme(theme);
    void poll();
    const p = setInterval(poll, 5000);
    const r = setInterval(() => (page = page + 1), rotateSecs * 1000);
    return () => {
      clearInterval(p);
      clearInterval(r);
    };
  });

  const hosts = $derived(
    [...(wall?.hosts ?? [])].sort((a, b) => statusRank(a.status) - statusRank(b.status) || a.name.localeCompare(b.name)),
  );
  const groups = $derived.by(() => {
    const names = [...new Set(hosts.flatMap((h) => (h.groups.length ? h.groups : ['ungrouped'])))].sort();
    return names.map((g) => ({ name: g, hosts: hosts.filter((h) => (h.groups.length ? h.groups.includes(g) : g === 'ungrouped')) }));
  });
  // Show all groups when they fit; otherwise rotate one group at a time.
  const rotating = $derived(hosts.length > 24 && groups.length > 1);
  const visibleGroups = $derived(rotating ? [groups[page % groups.length]!] : groups);
  const totals = $derived({
    critical: hosts.filter((h) => statusInfo(h.status).tone === 'critical').length,
    warning: hosts.filter((h) => statusInfo(h.status).tone === 'warning').length,
    ok: hosts.filter((h) => h.status === 'ok').length,
  });
  const stale = $derived(!!error && lastOk > 0);
</script>

<div class="wall" class:stale>
  <header>
    <span class="brand">{PRODUCT_NAME}</span>
    {#if rotating && visibleGroups[0]}<span class="group-name">{visibleGroups[0].name}<span class="pos"> · {(page % groups.length) + 1}/{groups.length}</span></span>{/if}
    <span class="spacer"></span>
    <span class="total crit"><OctagonAlert size={22} aria-hidden="true" />{totals.critical} critical</span>
    <span class="total warn"><TriangleAlert size={22} aria-hidden="true" />{totals.warning} warning</span>
    <span class="total ok"><CircleCheck size={22} aria-hidden="true" />{totals.ok} ok</span>
    <span class="clock num">{time(clock.now)}</span>
  </header>

  {#if error}
    <div class="notice" role="alert">
      <Unplug size={18} aria-hidden="true" />
      {error}{#if lastOk}<span> Data shown is from {time(lastOk)}.</span>{/if}
    </div>
  {/if}

  <div class="content">
    <div class="groups">
      {#each visibleGroups as g (g.name)}
        <section>
          {#if !rotating}<h2>{g.name}</h2>{/if}
          <div class="tiles">
            {#each g.hosts as h (h.name)}
              {@const info = statusInfo(h.status)}
              <div class={`tile tone-${info.tone}`} class:dim={h.stale}>
                <div class="t-head">
                  <span class="name mono">{h.name}</span>
                  <StatusBadge status={h.status} />
                </div>
                <div class="t-metrics">
                  <span><span class="k">CPU</span><span class="v num">{pct(h.cpu_pct, 0)}</span></span>
                  <span><span class="k">MEM</span><span class="v num">{pct(h.mem_pct, 0)}</span></span>
                  <span><span class="k">DISK</span><span class="v num">{pct(h.disk_pct, 0)}</span></span>
                </div>
                {#if h.alerts.critical + h.alerts.warning > 0}
                  <div class="t-alerts">
                    {#if h.alerts.critical}{h.alerts.critical} critical{/if}{#if h.alerts.critical && h.alerts.warning} · {/if}{#if h.alerts.warning}{h.alerts.warning} warning{/if}
                  </div>
                {:else if h.stale}
                  <div class="t-alerts">stale</div>
                {/if}
              </div>
            {/each}
          </div>
        </section>
      {/each}
      {#if wall && hosts.length === 0}<p class="none">No hosts in the groups this display can see.</p>{/if}
    </div>
    <aside>
      <h2>Firing alerts</h2>
      {#if !wall}
        <p class="muted">Loading…</p>
      {:else if wall.alerts.length === 0}
        <p class="quiet"><CircleCheck size={18} aria-hidden="true" />Nothing is firing.</p>
      {:else}
        <ul>
          {#each wall.alerts.slice(0, 14) as a, i (i)}
            <li class={`sev-${a.severity}`} class:acked={a.acknowledged}>
              <span class="sev">
                {#if a.severity === 'critical'}<OctagonAlert size={16} aria-hidden="true" />{:else}<TriangleAlert size={16} aria-hidden="true" />{/if}
                {a.severity}
              </span>
              <span class="mono host">{a.host ?? ''}</span>
              <span class="sum">{a.summary}</span>
              <span class="since num">{duration(clock.now - a.started_at)}{a.acknowledged ? ' · acknowledged' : ''}</span>
            </li>
          {/each}
        </ul>
        {#if wall.alerts.length > 14}<p class="muted">and {wall.alerts.length - 14} more</p>{/if}
      {/if}
    </aside>
  </div>
</div>

<style>
  .wall {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    color: var(--text);
    font-size: 16px;
    padding: 20px 28px;
    gap: 16px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 28px;
  }
  .brand {
    font-weight: 600;
    font-size: 20px;
    color: var(--text-2);
  }
  .group-name {
    font-size: 22px;
    font-weight: 600;
  }
  .pos {
    color: var(--text-3);
    font-weight: 400;
  }
  .spacer {
    flex: 1;
  }
  .total {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 20px;
    font-weight: 600;
  }
  .total.crit :global(svg) {
    color: var(--critical);
  }
  .total.warn :global(svg) {
    color: var(--warning);
  }
  .total.ok :global(svg) {
    color: var(--ok);
  }
  .clock {
    font-size: 24px;
    font-weight: 600;
    font-family: var(--font-mono);
  }
  .notice {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border: 1px solid var(--warning);
    border-left-width: 4px;
    border-radius: var(--radius);
    background: var(--warning-wash);
  }
  .stale .groups {
    opacity: 0.55;
  }
  .content {
    display: grid;
    grid-template-columns: 1fr 380px;
    gap: 24px;
    flex: 1;
    min-height: 0;
  }
  .groups {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  h2 {
    font-size: 15px;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 8px;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 10px;
  }
  .tile {
    padding: 12px 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 6px solid var(--unknown);
    border-radius: var(--radius);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .tile.tone-ok {
    border-left-color: var(--ok);
  }
  .tile.tone-warning {
    border-left-color: var(--warning);
  }
  .tile.tone-critical {
    border-left-color: var(--critical);
    background: var(--critical-wash);
  }
  .tile.dim {
    opacity: 0.6;
  }
  .t-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .t-head :global(.badge) {
    font-size: 14px;
  }
  .name {
    font-size: 17px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .t-metrics {
    display: flex;
    justify-content: space-between;
  }
  .t-metrics > span {
    display: flex;
    flex-direction: column;
  }
  .k {
    font-size: 11px;
    color: var(--text-3);
    letter-spacing: 0.05em;
  }
  .v {
    font-size: 20px;
    font-weight: 600;
  }
  .t-alerts {
    font-size: 14px;
    font-weight: 600;
  }
  aside {
    border-left: 1px solid var(--border);
    padding-left: 20px;
  }
  aside ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  aside li {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 10px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  aside li.acked {
    opacity: 0.65;
  }
  .sev {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
    text-transform: capitalize;
  }
  .sev-critical .sev :global(svg) {
    color: var(--critical);
  }
  .sev-warning .sev :global(svg) {
    color: var(--warning);
  }
  .host {
    text-align: right;
    font-size: 14px;
  }
  .sum {
    grid-column: 1 / -1;
    font-size: 15px;
  }
  .since {
    grid-column: 1 / -1;
    font-size: 13px;
    color: var(--text-3);
  }
  .quiet {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
  }
  .quiet :global(svg) {
    color: var(--ok);
  }
  .none {
    color: var(--text-3);
  }
  @media (max-width: 1100px) {
    .content {
      grid-template-columns: 1fr;
    }
    aside {
      border-left: 0;
      padding-left: 0;
    }
    header {
      flex-wrap: wrap;
      gap: 12px 20px;
    }
  }
</style>
