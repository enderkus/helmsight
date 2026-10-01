<script lang="ts">
  import { ArrowLeft, GitCompareArrows, Play, KeyRound } from '@lucide/svelte';
  import StatusBadge from '../components/StatusBadge.svelte';
  import LastUpdated from '../components/LastUpdated.svelte';
  import Tabs from '../components/Tabs.svelte';
  import SkeletonRows from '../components/SkeletonRows.svelte';
  import ErrorState from '../components/ErrorState.svelte';
  import Overview from './host/Overview.svelte';
  import Processes from './host/Processes.svelte';
  import NetworkTab from './host/Network.svelte';
  import Services from './host/Services.svelte';
  import Containers from './host/Containers.svelte';
  import System from './host/System.svelte';
  import ChangesTab from './host/Changes.svelte';
  import SecurityTab from './host/Security.svelte';
  import { api, ApiError, seg } from '../lib/api';
  import { fleet } from '../lib/fleet.svelte';
  import { router } from '../lib/router.svelte';
  import { session, can } from '../lib/session.svelte';
  import { statusInfo } from '../lib/status';
  import { duration } from '../lib/format';
  import type { HostDetail } from '../lib/types';

  interface Props {
    params: Record<string, string>;
  }
  let { params }: Props = $props();
  const name = $derived(params.name ?? '');

  let detail = $state<HostDetail | null>(null);
  let error = $state<string | null>(null);
  let notFound = $state(false);
  let loadedFor = '';
  const tab = $derived(router.query.get('tab') ?? 'overview');

  const summary = $derived(fleet.hosts[name] ?? detail?.summary ?? null);
  const liveTick = $derived(fleet.hosts[name]?.last_attempt ?? 0);

  async function load(n: string) {
    try {
      const d = await api.get<HostDetail>(`/hosts/${seg(n)}`);
      if (n === name) {
        detail = d;
        error = null;
        notFound = false;
      }
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) notFound = true;
      else error = e instanceof ApiError ? e.message : 'Cannot load host.';
    }
  }

  $effect(() => {
    const n = name;
    void liveTick;
    if (n !== loadedFor) {
      detail = null;
      error = null;
      notFound = false;
      loadedFor = n;
    }
    void load(n);
  });

  const counts = $derived.by(() => {
    const m = detail?.medium?.data;
    const svc = m?.services.status === 'ok' ? m.services.data : [];
    const failed = svc.filter((s) => s.active === 'failed' || s.sub === 'failed' || s.sub === 'crashed').length;
    const containers = m?.containers.status === 'ok' ? m.containers.data.length : null;
    return { failed, containers };
  });

  const tabs = $derived([
    { id: 'overview', label: 'Overview' },
    { id: 'processes', label: 'Processes' },
    { id: 'network', label: 'Network' },
    { id: 'services', label: 'Services', count: counts.failed || null, alert: counts.failed > 0 },
    { id: 'containers', label: 'Containers', count: counts.containers },
    { id: 'system', label: 'System' },
    { id: 'changes', label: 'Changes' },
    { id: 'security', label: 'Security' },
  ]);

  const connectionProblem = $derived(summary && summary.connection !== 'ok' && summary.connection !== 'pending' && summary.connection !== 'disabled');
</script>

<div class="page">
  <a class="back" href="/"><ArrowLeft size={14} />Fleet</a>
  {#if notFound}
    <ErrorState message={`There is no host named "${name}". It may have been removed from the configuration.`} />
  {:else if !summary}
    {#if error}<ErrorState message={error} onretry={() => void load(name)} />{:else}<div class="panel"><SkeletonRows rows={6} /></div>{/if}
  {:else}
    <header class="host-head">
      <div class="title">
        <h1 class="mono">{summary.name}</h1>
        <StatusBadge status={summary.status} />
        <LastUpdated ts={summary.last_success} stale={summary.stale} />
      </div>
      <div class="facts">
        <span class="mono">{summary.address}{summary.port !== 22 ? `:${summary.port}` : ''}</span>
        {#if summary.os}<span>{summary.os}</span>{/if}
        {#if summary.kernel}<span class="mono">{summary.kernel}</span>{/if}
        {#if summary.uptime_secs !== null}<span>up {duration(summary.uptime_secs)}</span>{/if}
        {#each summary.groups as g (g)}<a class="tag" href={`/?group=${encodeURIComponent(g)}`}>{g}</a>{/each}
        {#each summary.tags as t (t)}<a class="tag mono" href={`/?tag=${encodeURIComponent(t)}`}>{t}</a>{/each}
      </div>
      <div class="head-actions">
        <a class="btn small" href={`/compare?a=${encodeURIComponent(summary.name)}${summary.baseline ? '' : ''}`}>
          <GitCompareArrows size={14} />{summary.baseline ? 'Compare with baseline' : 'Compare'}
        </a>
        {#if session.meta?.features.actions && can('operator')}
          <a class="btn small" href={`/actions?host=${encodeURIComponent(summary.name)}`}><Play size={14} />Actions</a>
        {/if}
      </div>
    </header>

    {#if connectionProblem}
      <div class="banner critical" role="alert">
        <div class="body">
          <p class="title">{statusInfo(summary.status).label}: {statusInfo(summary.status).hint}</p>
          {#if summary.error}<p class="mono err">{summary.error}</p>{/if}
          {#if summary.host_key}
            <p>
              Presented key: <span class="mono">{summary.host_key.algorithm} {summary.host_key.fingerprint}</span>
              {#if summary.host_key.previous}<br />Previously trusted: <span class="mono">{summary.host_key.previous}</span>{/if}
            </p>
          {/if}
          <p class="muted">Data below is from the last successful collection.</p>
        </div>
        {#if summary.host_key && can('admin')}
          <a class="btn small" href="/admin/hostkeys"><KeyRound size={14} />Review key</a>
        {/if}
      </div>
    {:else if summary.partial}
      <div class="banner warning">
        <div class="body">
          <p class="title">Collection partially failed</p>
          <p>Not collected: <span class="mono">{summary.missing.join(', ') || 'output truncated'}</span>. Other data is current.</p>
        </div>
      </div>
    {/if}

    <Tabs {tabs} active={tab} label="Host sections" onselect={(id) => router.setQuery({ tab: id === 'overview' ? null : id })} />

    {#if error && !detail}
      <ErrorState message={error} onretry={() => void load(name)} />
    {:else if tab === 'overview'}
      <Overview host={name} {detail} {summary} />
    {:else if !detail}
      <div class="panel"><SkeletonRows rows={8} /></div>
    {:else if tab === 'processes'}
      <Processes {detail} />
    {:else if tab === 'network'}
      <NetworkTab {detail} />
    {:else if tab === 'services'}
      <Services {detail} />
    {:else if tab === 'containers'}
      <Containers {detail} />
    {:else if tab === 'system'}
      <System {detail} />
    {:else if tab === 'changes'}
      <ChangesTab host={name} />
    {:else if tab === 'security'}
      <SecurityTab host={name} />
    {/if}
  {/if}
</div>

<style>
  .back {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--text-3);
    margin-bottom: 8px;
  }
  .host-head {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 6px 16px;
    margin-bottom: 12px;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .title h1 {
    font-size: 18px;
    font-weight: 600;
  }
  .facts {
    grid-column: 1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 14px;
    color: var(--text-2);
    font-size: 12.5px;
  }
  .facts .tag:hover {
    text-decoration: none;
    border-color: var(--border-strong);
  }
  .head-actions {
    grid-column: 2;
    grid-row: 1 / span 2;
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }
  .banner {
    margin-bottom: 12px;
  }
  .err {
    color: var(--critical-text);
    margin: 2px 0;
    font-size: 12px;
  }
  @media (max-width: 700px) {
    .host-head {
      grid-template-columns: 1fr;
    }
    .head-actions {
      grid-column: 1;
      grid-row: auto;
    }
  }
</style>
