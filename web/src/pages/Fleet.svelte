<script lang="ts">
  import { onMount } from 'svelte';
  import { Search, Rows3, LayoutGrid, ArrowUp, ArrowDown, X } from '@lucide/svelte';
  import StatusBadge from '../components/StatusBadge.svelte';
  import Sparkline from '../components/Sparkline.svelte';
  import Meter from '../components/Meter.svelte';
  import LastUpdated from '../components/LastUpdated.svelte';
  import SkeletonRows from '../components/SkeletonRows.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import ErrorState from '../components/ErrorState.svelte';
  import { fleet, loadFleet } from '../lib/fleet.svelte';
  import { router } from '../lib/router.svelte';
  import { statusInfo, statusRank } from '../lib/status';
  import { duration, pct, rate, num, bytes } from '../lib/format';
  import type { HostSummary } from '../lib/types';

  interface Props {
    params?: Record<string, string>;
  }
  let { params: _params = {} }: Props = $props();

  type SortKey = 'status' | 'name' | 'cpu' | 'mem' | 'load' | 'disk' | 'net' | 'uptime' | 'alerts';
  type Filter = 'all' | 'attention' | 'ok' | 'warning' | 'critical' | 'down' | 'pending';

  const q = router.query;
  let search = $state(q.get('q') ?? '');
  let filter = $state<Filter>((q.get('status') as Filter) ?? 'all');
  let group = $state(q.get('group') ?? '');
  let tag = $state(q.get('tag') ?? '');
  let sort = $state<SortKey>((q.get('sort') as SortKey) ?? 'status');
  let desc = $state(q.get('dir') === 'desc');
  let view = $state<'table' | 'grid'>(
    (q.get('view') as 'table' | 'grid') ??
      ((typeof localStorage !== 'undefined' && (localStorage.getItem('fleet.view') as 'table' | 'grid')) ||
        (typeof window !== 'undefined' && window.innerWidth < 700 ? 'grid' : 'table')),
  );
  let searchEl = $state<HTMLInputElement | undefined>();

  onMount(() => {
    if (router.query.get('focus') === 'search') {
      searchEl?.focus();
      router.setQuery({ focus: null });
    }
  });

  $effect(() => {
    router.setQuery({
      q: search || null,
      status: filter === 'all' ? null : filter,
      group: group || null,
      tag: tag || null,
      sort: sort === 'status' ? null : sort,
      dir: desc ? 'desc' : null,
      view: view === 'table' ? null : view,
    });
    try {
      localStorage.setItem('fleet.view', view);
    } catch {
      // ignore
    }
  });

  const hosts = $derived(Object.values(fleet.hosts));
  const down = (h: HostSummary) =>
    ['unreachable', 'auth_failed', 'host_key_changed', 'host_key_unknown'].includes(h.status);

  function inFilter(h: HostSummary, f: Filter): boolean {
    switch (f) {
      case 'all':
        return true;
      case 'attention':
        return h.status !== 'ok' && h.status !== 'disabled' || h.partial || h.stale;
      case 'down':
        return down(h);
      case 'pending':
        return h.status === 'pending';
      default:
        return h.status === f;
    }
  }

  const counts = $derived({
    all: hosts.length,
    attention: hosts.filter((h) => inFilter(h, 'attention')).length,
    ok: hosts.filter((h) => h.status === 'ok').length,
    warning: hosts.filter((h) => h.status === 'warning').length,
    critical: hosts.filter((h) => h.status === 'critical').length,
    down: hosts.filter(down).length,
    pending: hosts.filter((h) => h.status === 'pending').length,
  });

  const groups = $derived([...new Set(hosts.flatMap((h) => h.groups))].sort());
  const tags = $derived([...new Set(hosts.flatMap((h) => h.tags))].sort());

  function matchesSearch(h: HostSummary, s: string): boolean {
    const terms = s.toLowerCase().split(/\s+/).filter(Boolean);
    return terms.every((t) => {
      if (t.startsWith('group:')) return h.groups.some((g) => g.toLowerCase() === t.slice(6));
      if (t.startsWith('tag:')) return h.tags.some((x) => x.toLowerCase() === t.slice(4));
      const hay = [h.name, h.address, h.os ?? '', h.kernel ?? '', ...h.groups, ...h.tags].join(' ').toLowerCase();
      return hay.includes(t);
    });
  }

  function sortValue(h: HostSummary, k: SortKey): number | string {
    switch (k) {
      case 'status':
        return statusRank(h.status) * 1000 - (h.alerts.critical * 10 + h.alerts.warning);
      case 'name':
        return h.name;
      case 'cpu':
        return h.cpu_pct ?? -1;
      case 'mem':
        return h.mem_pct ?? -1;
      case 'load':
        return h.load1 !== null && h.cores ? h.load1 / h.cores : -1;
      case 'disk':
        return h.disk?.used_pct ?? -1;
      case 'net':
        return (h.net_rx ?? 0) + (h.net_tx ?? 0);
      case 'uptime':
        return h.uptime_secs ?? -1;
      case 'alerts':
        return h.alerts.critical * 1000 + h.alerts.warning;
    }
  }

  const visible = $derived.by(() => {
    const list = hosts.filter(
      (h) =>
        inFilter(h, filter) &&
        (!group || h.groups.includes(group)) &&
        (!tag || h.tags.includes(tag)) &&
        matchesSearch(h, search),
    );
    const numericDefaultDesc = sort !== 'name' && sort !== 'status';
    const dir = (numericDefaultDesc ? !desc : desc) ? -1 : 1;
    return list.sort((a, b) => {
      const x = sortValue(a, sort);
      const y = sortValue(b, sort);
      const c = typeof x === 'string' ? x.localeCompare(y as string) : (x as number) - (y as number);
      return c !== 0 ? c * dir : a.name.localeCompare(b.name);
    });
  });

  function setSort(k: SortKey) {
    if (sort === k) desc = !desc;
    else {
      sort = k;
      desc = false;
    }
  }

  function clearFilters() {
    search = '';
    filter = 'all';
    group = '';
    tag = '';
  }

  const filters: [Filter, string][] = [
    ['all', 'All'],
    ['attention', 'Needs attention'],
    ['critical', 'Critical'],
    ['warning', 'Warning'],
    ['down', 'Not collecting'],
    ['ok', 'OK'],
    ['pending', 'Pending'],
  ];

  const columns: [SortKey, string, boolean][] = [
    ['status', 'Status', false],
    ['name', 'Host', false],
    ['cpu', 'CPU', true],
    ['mem', 'Memory', true],
    ['load', 'Load', true],
    ['disk', 'Disk (fullest)', true],
    ['net', 'Network', true],
    ['uptime', 'Uptime', true],
    ['alerts', 'Alerts', true],
  ];

  const cpuSpark = (h: HostSummary) => (h.spark ?? []).slice(-60).map((p) => p.cpu);
  const memSpark = (h: HostSummary) => (h.spark ?? []).slice(-60).map((p) => p.mem);
  const hasFilters = $derived(!!search || filter !== 'all' || !!group || !!tag);
</script>

<div class="page">
  <div class="page-header">
    <h1>Fleet</h1>
    <span class="muted">{hosts.length} {hosts.length === 1 ? 'host' : 'hosts'}</span>
    <span class="spacer"></span>
    <div class="segmented" role="group" aria-label="View">
      <button type="button" aria-pressed={view === 'table'} onclick={() => (view = 'table')} title="Table view">
        <Rows3 size={14} aria-hidden="true" />Table
      </button>
      <button type="button" aria-pressed={view === 'grid'} onclick={() => (view = 'grid')} title="Grid view">
        <LayoutGrid size={14} aria-hidden="true" />Grid
      </button>
    </div>
  </div>

  <div class="filters">
    <div class="chips" role="group" aria-label="Filter by status">
      {#each filters as [f, label] (f)}
        {#if f === 'all' || f === 'attention' || counts[f] > 0}
          <button type="button" class="chip" aria-pressed={filter === f} onclick={() => (filter = f)}>
            {label}<span class="n">{counts[f]}</span>
          </button>
        {/if}
      {/each}
    </div>
    <div class="controls">
      <label class="search">
        <Search size={14} aria-hidden="true" />
        <span class="sr-only">Search hosts</span>
        <input
          class="input"
          data-search
          bind:this={searchEl}
          bind:value={search}
          placeholder="Search name, address, OS, tag:env:prod"
          onkeydown={(e) => {
            if (e.key === 'Escape') {
              search = '';
              (e.target as HTMLInputElement).blur();
            }
          }}
        />
        <kbd aria-hidden="true">/</kbd>
      </label>
      {#if groups.length}
        <label class="sr-only" for="grp">Group</label>
        <select id="grp" class="select" bind:value={group}>
          <option value="">All groups</option>
          {#each groups as g (g)}<option value={g}>{g}</option>{/each}
        </select>
      {/if}
      {#if tags.length}
        <label class="sr-only" for="tg">Tag</label>
        <select id="tg" class="select" bind:value={tag}>
          <option value="">All tags</option>
          {#each tags as t (t)}<option value={t}>{t}</option>{/each}
        </select>
      {/if}
      {#if hasFilters}
        <button class="btn ghost small" type="button" onclick={clearFilters}><X size={14} />Clear</button>
      {/if}
    </div>
  </div>

  {#if fleet.error && !fleet.loaded}
    <ErrorState message={fleet.error} onretry={() => void loadFleet()} />
  {:else if !fleet.loaded}
    <div class="panel"><SkeletonRows rows={8} /></div>
  {:else if hosts.length === 0}
    <div class="panel">
      <EmptyState title="No hosts configured">
        <p>Add <code>[[hosts]]</code> entries to the configuration file, run <code>hosts test --trust</code> and restart the server.</p>
      </EmptyState>
    </div>
  {:else if visible.length === 0}
    <div class="panel">
      <EmptyState title="No hosts match these filters">
        <button class="btn small" type="button" onclick={clearFilters}>Clear filters</button>
      </EmptyState>
    </div>
  {:else if view === 'table'}
    {#if fleet.error}<ErrorState message={fleet.error} onretry={() => void loadFleet()} />{/if}
    <div class="panel table-wrap fleet-table">
      <table class="data">
        <thead>
          <tr>
            {#each columns as [k, label, numeric] (k)}
              <th class:num={numeric} aria-sort={sort === k ? (desc ? 'descending' : 'ascending') : 'none'}>
                <button class="sort" type="button" onclick={() => setSort(k)}>
                  {label}
                  {#if sort === k}
                    {#if desc}<ArrowDown size={12} />{:else}<ArrowUp size={12} />{/if}
                  {/if}
                </button>
              </th>
            {/each}
            <th class="num">Updated</th>
          </tr>
        </thead>
        <tbody>
          {#each visible as h (h.name)}
            {@const collecting = h.connection === 'ok'}
            <tr class:stale-row={h.stale}>
              <td><StatusBadge status={h.status} /></td>
              <td class="host-cell">
                <a class="mono host" href={`/hosts/${encodeURIComponent(h.name)}`}>{h.name}</a>
                {#if h.error && !collecting}
                  <span class="err truncate" title={h.error}>{h.error}</span>
                {:else if h.partial}
                  <span class="partial" title={`Not collected: ${h.missing.join(', ')}`}>partial</span>
                {:else}
                  <span class="muted sub truncate">{h.os ?? h.address}</span>
                {/if}
              </td>
              <td class="num">
                <span class="metric">
                  <Sparkline values={cpuSpark(h)} width={56} label={`CPU of ${h.name}, last 15 minutes`} />
                  <span title={h.iowait_pct !== null ? `iowait ${pct(h.iowait_pct)}` : undefined}>{pct(h.cpu_pct)}</span>
                </span>
              </td>
              <td class="num">
                <span class="metric">
                  <Sparkline values={memSpark(h)} width={56} label={`Memory of ${h.name}, last 15 minutes`} />
                  <span title={h.mem_total !== null ? `of ${bytes(h.mem_total)}` : undefined}>{pct(h.mem_pct)}</span>
                </span>
              </td>
              <td class="num" title={h.cores ? `${h.cores} cores; 5m ${num(h.load5)}, 15m ${num(h.load15)}` : undefined}>
                {num(h.load1)}{#if h.cores}<span class="muted"> /{h.cores}</span>{/if}
              </td>
              <td class="num">
                {#if h.disk}
                  <span class="disk" title={`${h.disk.mount}: ${bytes(h.disk.avail)} free`}>
                    <span class="mono mount truncate">{h.disk.mount}</span>
                    <Meter value={h.disk.used_pct} label={`Disk ${h.disk.mount}`} width={48} />
                  </span>
                {:else}–{/if}
              </td>
              <td class="num nowrap net">
                {#if h.net_rx !== null}
                  <span title="Received">↓ {rate(h.net_rx)}</span>
                  <span title="Sent">↑ {rate(h.net_tx)}</span>
                {:else}–{/if}
              </td>
              <td class="num nowrap">{duration(h.uptime_secs)}</td>
              <td class="num">
                {#if h.alerts.critical + h.alerts.warning > 0}
                  <a class="alerts" href={`/alerts?host=${encodeURIComponent(h.name)}`}>
                    {#if h.alerts.critical}<span class="a crit">{h.alerts.critical} critical</span>{/if}
                    {#if h.alerts.warning}<span class="a warn">{h.alerts.warning} warning</span>{/if}
                  </a>
                {:else}<span class="muted">0</span>{/if}
              </td>
              <td class="num"><LastUpdated ts={h.last_success} stale={h.stale} prefix="" /></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="grid">
      {#each visible as h (h.name)}
        {@const info = statusInfo(h.status)}
        <a class={`card panel tone-${info.tone}`} class:stale-row={h.stale} href={`/hosts/${encodeURIComponent(h.name)}`}>
          <div class="card-head">
            <span class="mono host">{h.name}</span>
            <StatusBadge status={h.status} />
          </div>
          <p class="muted sub truncate">{h.os ?? h.address}</p>
          {#if h.connection !== 'ok' && h.error}
            <p class="err clamp">{h.error}</p>
          {:else}
            <div class="card-metrics">
              <div><span class="k">CPU</span><Sparkline values={cpuSpark(h)} width={90} label={`CPU of ${h.name}`} /><span class="v num">{pct(h.cpu_pct)}</span></div>
              <div><span class="k">Memory</span><Sparkline values={memSpark(h)} width={90} label={`Memory of ${h.name}`} /><span class="v num">{pct(h.mem_pct)}</span></div>
              <div>
                <span class="k">Disk</span>
                {#if h.disk}<Meter value={h.disk.used_pct} width={90} label={`Disk ${h.disk.mount}`} showValue={false} /><span class="v num">{pct(h.disk.used_pct)}</span>{:else}<span class="v">–</span>{/if}
              </div>
            </div>
          {/if}
          <div class="card-foot">
            {#if h.alerts.critical + h.alerts.warning > 0}
              <span class="alerts">
                {#if h.alerts.critical}<span class="a crit">{h.alerts.critical} critical</span>{/if}
                {#if h.alerts.warning}<span class="a warn">{h.alerts.warning} warning</span>{/if}
              </span>
            {:else}<span class="muted">No alerts</span>{/if}
            <span class="spacer"></span>
            <LastUpdated ts={h.last_success} stale={h.stale} prefix="" />
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
    margin-bottom: 12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 13px;
    background: var(--surface);
    color: var(--text-2);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .chip[aria-pressed='true'] {
    background: var(--text);
    border-color: var(--text);
    color: var(--surface);
  }
  .chip .n {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
  .controls {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
  }
  .search :global(svg) {
    position: absolute;
    left: 8px;
    color: var(--text-3);
    pointer-events: none;
  }
  .search .input {
    padding-left: 28px;
    padding-right: 28px;
    width: 280px;
  }
  .search kbd {
    position: absolute;
    right: 6px;
    pointer-events: none;
  }
  .fleet-table {
    max-height: calc(100vh - 180px);
  }
  .host-cell {
    max-width: 240px;
  }
  .fleet-table :global(td),
  .fleet-table :global(th) {
    padding: 0 8px;
  }
  .host {
    display: block;
    font-weight: 500;
    color: var(--text);
  }
  .host-cell .host:hover {
    color: var(--accent-text);
  }
  .sub {
    display: block;
    font-size: 11.5px;
  }
  .err {
    display: block;
    color: var(--critical-text);
    font-size: 11.5px;
  }
  .partial {
    display: inline-block;
    margin-top: 1px;
    padding: 0 5px;
    border: 1px solid var(--warning);
    border-radius: var(--radius-sm);
    background: var(--warning-wash);
    font-size: 11px;
  }
  tbody td {
    height: 42px;
  }
  .metric {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    justify-content: flex-end;
  }
  .metric > span {
    min-width: 44px;
    text-align: right;
  }
  .disk {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .mount {
    max-width: 90px;
    color: var(--text-3);
    font-size: 11px;
  }
  .net {
    font-size: 12px;
  }
  .net span {
    display: block;
    line-height: 16px;
  }
  .alerts {
    display: inline-flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 1px;
    color: var(--text);
    font-size: 12px;
  }
  .a {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
  }
  .a::before {
    content: '';
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .a.crit::before {
    background: var(--critical);
  }
  .a.warn::before {
    background: var(--warning);
  }
  .stale-row :global(td),
  .card.stale-row {
    opacity: 0.6;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 10px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    color: var(--text);
    border-top: 3px solid var(--border);
  }
  .card:hover {
    text-decoration: none;
    border-color: var(--border-strong);
  }
  .card.tone-critical {
    border-top-color: var(--critical);
  }
  .card.tone-warning {
    border-top-color: var(--warning);
  }
  .card.tone-ok {
    border-top-color: var(--ok);
  }
  .card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .card-metrics {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 4px 0;
  }
  .card-metrics div {
    display: grid;
    grid-template-columns: 56px 1fr 52px;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .card-metrics .k {
    color: var(--text-3);
  }
  .card-metrics .v {
    text-align: right;
  }
  .card-foot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-top: 6px;
    border-top: 1px solid var(--border);
    font-size: 12px;
  }
  .card-foot .alerts {
    flex-direction: row;
    gap: 10px;
  }
  .card-foot .spacer {
    flex: 1;
  }
  .clamp {
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  @media (max-width: 700px) {
    .search .input {
      width: 100%;
    }
    .search {
      flex: 1;
    }
    .controls {
      width: 100%;
    }
  }
</style>
