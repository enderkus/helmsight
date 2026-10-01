<script lang="ts">
  import { Check, BellOff } from '@lucide/svelte';
  import Tabs from '../components/Tabs.svelte';
  import SeverityBadge from '../components/SeverityBadge.svelte';
  import Dialog from '../components/Dialog.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import ErrorState from '../components/ErrorState.svelte';
  import SkeletonRows from '../components/SkeletonRows.svelte';
  import { api, ApiError, qs } from '../lib/api';
  import { fleet } from '../lib/fleet.svelte';
  import { router } from '../lib/router.svelte';
  import { can } from '../lib/session.svelte';
  import { clock } from '../lib/clock.svelte';
  import { toast } from '../lib/toast.svelte';
  import { ago, datetime, duration } from '../lib/format';
  import type { Alert, RulesView, Silence } from '../lib/types';

  interface Props {
    params?: Record<string, string>;
  }
  let { params: _params = {} }: Props = $props();

  const tab = $derived(router.query.get('tab') ?? 'firing');
  const hostFilter = $derived(router.query.get('host') ?? '');
  let alerts = $state<Alert[] | null>(null);
  let silences = $state<Silence[] | null>(null);
  let rules = $state<RulesView | null>(null);
  let error = $state<string | null>(null);

  let ackFor = $state<Alert | null>(null);
  let ackNote = $state('');
  let silenceOpen = $state(false);
  let sRule = $state('');
  let sHost = $state('');
  let sDuration = $state('2h');
  let sReason = $state('');
  let busy = $state(false);

  async function load() {
    try {
      const since = Math.floor(Date.now() / 1000) - 7 * 86400;
      [alerts, silences] = await Promise.all([
        api.get<Alert[]>(`/alerts${qs({ since })}`),
        api.get<Silence[]>('/silences'),
      ]);
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Cannot load alerts.';
    }
  }

  $effect(() => {
    void fleet.alertsVersion;
    void load();
  });
  $effect(() => {
    if (tab === 'rules' && !rules) {
      api.get<RulesView>('/rules').then((r) => (rules = r)).catch(() => (error = 'Cannot load rules.'));
    }
  });

  const filtered = $derived((alerts ?? []).filter((a) => !hostFilter || a.host === hostFilter));
  const firing = $derived(filtered.filter((a) => a.state === 'firing').sort((a, b) => (a.severity === b.severity ? b.started_at - a.started_at : a.severity === 'critical' ? -1 : 1)));
  const recent = $derived(filtered.filter((a) => a.state === 'resolved'));
  const activeSilences = $derived((silences ?? []).filter((s) => s.ends_at > clock.now));
  const endedSilences = $derived((silences ?? []).filter((s) => s.ends_at <= clock.now));

  async function ack(e: SubmitEvent) {
    e.preventDefault();
    if (!ackFor) return;
    busy = true;
    try {
      await api.post(`/alerts/${ackFor.id}/ack`, { note: ackNote || null });
      toast('Alert acknowledged. Reminders stop until it resolves.');
      ackFor = null;
      ackNote = '';
      await load();
    } catch (err) {
      toast(err instanceof ApiError ? err.message : 'Could not acknowledge.', 'error');
    } finally {
      busy = false;
    }
  }

  function openSilence(a?: Alert) {
    sRule = a?.rule_id ?? '';
    sHost = a?.host ?? hostFilter;
    sDuration = '2h';
    sReason = '';
    silenceOpen = true;
  }

  async function createSilence(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      await api.post('/silences', { rule_id: sRule || null, host: sHost || null, duration: sDuration, reason: sReason });
      toast('Silence created. Notifications are suppressed until it ends.');
      silenceOpen = false;
      await load();
    } catch (err) {
      toast(err instanceof ApiError ? err.message : 'Could not create the silence.', 'error');
    } finally {
      busy = false;
    }
  }

  async function expire(s: Silence) {
    try {
      await api.del(`/silences/${s.id}`);
      toast('Silence ended.');
      await load();
    } catch (err) {
      toast(err instanceof ApiError ? err.message : 'Could not end the silence.', 'error');
    }
  }

  const ruleIds = $derived([...new Set((alerts ?? []).map((a) => a.rule_id))].sort());
  const hostNames = $derived(Object.keys(fleet.hosts).sort());
</script>

<div class="page">
  <div class="page-header">
    <h1>Alerts</h1>
    {#if hostFilter}
      <span class="tag mono">{hostFilter}</span>
      <button class="btn ghost small" type="button" onclick={() => router.setQuery({ host: null })}>Show all hosts</button>
    {/if}
    <span class="spacer"></span>
    {#if can('operator')}
      <button class="btn" type="button" onclick={() => openSilence()}><BellOff size={14} />New silence</button>
    {/if}
  </div>

  <Tabs
    label="Alert views"
    active={tab}
    onselect={(id) => router.setQuery({ tab: id === 'firing' ? null : id })}
    tabs={[
      { id: 'firing', label: 'Firing', count: alerts ? firing.length : null, alert: firing.some((a) => a.severity === 'critical') },
      { id: 'recent', label: 'Resolved (7 days)', count: alerts ? recent.length : null },
      { id: 'silences', label: 'Silences', count: silences ? activeSilences.length : null },
      { id: 'rules', label: 'Rules' },
    ]}
  />

  {#if error}
    <ErrorState message={error} onretry={load} />
  {:else if tab === 'rules'}
    {#if !rules}<div class="panel"><SkeletonRows /></div>
    {:else}
      <div class="stack">
        <section class="panel">
          <header class="panel-header"><h2>Rules</h2><span class="muted">Defined in the configuration file</span></header>
          <table class="data">
            <thead><tr><th>Id</th><th>Condition</th><th>Severity</th><th>Scope</th></tr></thead>
            <tbody>
              {#each rules.rules as r (r.id)}
                <tr>
                  <td class="mono">{r.id}{#if r.builtin}<span class="muted"> · built-in</span>{/if}</td>
                  <td class="mono">{r.expr}{#if r.summary}<br /><span class="muted sans">{r.summary}</span>{/if}</td>
                  <td><SeverityBadge severity={r.severity} /></td>
                  <td class="muted">{[...r.hosts, ...r.groups.map((g) => `group ${g}`), ...r.tags.map((t) => `tag ${t}`)].join(', ') || 'all hosts'}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </section>
        <section class="panel">
          <header class="panel-header"><h2>Metrics available to rules</h2></header>
          <table class="data">
            <thead><tr><th>Metric</th><th>Unit</th><th>Per</th><th>Description</th></tr></thead>
            <tbody>
              {#each rules.metrics as m (m.name)}
                <tr><td class="mono">{m.name}</td><td>{m.unit || '–'}</td><td class="muted">{m.instance ?? 'host'}</td><td class="secondary">{m.description}</td></tr>
              {/each}
            </tbody>
          </table>
        </section>
      </div>
    {/if}
  {:else if alerts === null}
    <div class="panel"><SkeletonRows rows={6} /></div>
  {:else if tab === 'firing'}
    <section class="panel">
      {#if firing.length === 0}
        <EmptyState title="Nothing is firing">All alert conditions are clear{hostFilter ? ' for this host' : ''}.</EmptyState>
      {:else}
        <div class="table-wrap">
          <table class="data">
            <thead><tr><th>Severity</th><th>Host</th><th>Alert</th><th>Since</th><th>State</th>{#if can('operator')}<th class="num">Actions</th>{/if}</tr></thead>
            <tbody>
              {#each firing as a (a.id)}
                <tr>
                  <td><SeverityBadge severity={a.severity} /></td>
                  <td>{#if a.host}<a class="mono" href={`/hosts/${encodeURIComponent(a.host)}`}>{a.host}</a>{:else}<span class="muted">–</span>{/if}</td>
                  <td class="summary">
                    <span>{a.summary}</span>
                    <span class="muted mono small">{a.rule_id}{a.instance ? ` · ${a.instance}` : ''}</span>
                  </td>
                  <td class="nowrap" title={datetime(a.started_at)}>{duration(clock.now - a.started_at)}</td>
                  <td class="nowrap small">
                    {#if a.ack_by}<span title={`${datetime(a.ack_at)}${a.ack_note ? ` · ${a.ack_note}` : ''}`}>Acknowledged by {a.ack_by}</span>
                    {:else if a.silenced}<span>Silenced</span>
                    {:else}<span class="muted">Open</span>{/if}
                  </td>
                  {#if can('operator')}
                    <td class="num nowrap">
                      {#if !a.ack_by}<button class="btn small" type="button" onclick={() => (ackFor = a)}><Check size={13} />Acknowledge</button>{/if}
                      <button class="btn small ghost" type="button" onclick={() => openSilence(a)} title="Silence similar alerts"><BellOff size={13} /><span class="sr-only">Silence</span></button>
                    </td>
                  {/if}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>
  {:else if tab === 'recent'}
    <section class="panel">
      {#if recent.length === 0}
        <EmptyState title="No resolved alerts in the last 7 days" />
      {:else}
        <div class="table-wrap">
          <table class="data">
            <thead><tr><th>Severity</th><th>Host</th><th>Alert</th><th>Started</th><th>Resolved</th><th class="num">Duration</th></tr></thead>
            <tbody>
              {#each recent as a (a.id)}
                <tr>
                  <td><SeverityBadge severity={a.severity} /></td>
                  <td>{#if a.host}<a class="mono" href={`/hosts/${encodeURIComponent(a.host)}`}>{a.host}</a>{:else}–{/if}</td>
                  <td class="summary"><span>{a.summary}</span><span class="muted mono small">{a.rule_id}{a.instance ? ` · ${a.instance}` : ''}</span></td>
                  <td class="nowrap">{datetime(a.started_at)}</td>
                  <td class="nowrap">{datetime(a.resolved_at)}</td>
                  <td class="num">{duration((a.resolved_at ?? clock.now) - a.started_at)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>
  {:else if tab === 'silences'}
    <section class="panel">
      {#if (silences ?? []).length === 0}
        <EmptyState title="No silences">Silences suppress notifications for matching alerts until they end.</EmptyState>
      {:else}
        <div class="table-wrap">
          <table class="data">
            <thead><tr><th>Matches</th><th>Reason</th><th>Created by</th><th>Ends</th>{#if can('operator')}<th></th>{/if}</tr></thead>
            <tbody>
              {#each [...activeSilences, ...endedSilences] as s (s.id)}
                <tr class:ended={s.ends_at <= clock.now}>
                  <td class="mono">{s.rule_id ?? 'any rule'} · {s.host ?? 'any host'}</td>
                  <td>{s.reason}</td>
                  <td>{s.created_by}</td>
                  <td class="nowrap" title={datetime(s.ends_at)}>
                    {s.ends_at > clock.now ? ago(s.ends_at, clock.now) : `ended ${ago(s.ends_at, clock.now)}${s.expired_by ? ` by ${s.expired_by}` : ''}`}
                  </td>
                  {#if can('operator')}
                    <td class="num">{#if s.ends_at > clock.now}<button class="btn small" type="button" onclick={() => expire(s)}>End now</button>{/if}</td>
                  {/if}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>
  {/if}
</div>

<Dialog open={!!ackFor} title="Acknowledge alert" onclose={() => (ackFor = null)}>
  {#if ackFor}
    <form id="ack-form" class="stack" onsubmit={ack}>
      <p>{ackFor.summary}</p>
      <label class="field">
        <span>Note (optional)</span>
        <textarea class="input" rows="3" maxlength="500" bind:value={ackNote} placeholder="What is being done about it"></textarea>
      </label>
    </form>
  {/if}
  {#snippet footer()}
    <button class="btn" type="button" onclick={() => (ackFor = null)}>Cancel</button>
    <button class="btn primary" type="submit" form="ack-form" disabled={busy}>Acknowledge</button>
  {/snippet}
</Dialog>

<Dialog open={silenceOpen} title="New silence" onclose={() => (silenceOpen = false)}>
  <form id="silence-form" class="stack" onsubmit={createSilence}>
    <label class="field">
      <span>Rule</span>
      <select class="select" bind:value={sRule}>
        <option value="">Any rule</option>
        {#each ruleIds as r (r)}<option value={r}>{r}</option>{/each}
      </select>
    </label>
    <label class="field">
      <span>Host</span>
      <select class="select" bind:value={sHost}>
        <option value="">Any host</option>
        {#each hostNames as h (h)}<option value={h}>{h}</option>{/each}
      </select>
    </label>
    <label class="field">
      <span>Duration</span>
      <select class="select" bind:value={sDuration}>
        <option value="30m">30 minutes</option><option value="2h">2 hours</option><option value="8h">8 hours</option>
        <option value="1d">1 day</option><option value="7d">7 days</option>
      </select>
    </label>
    <label class="field">
      <span>Reason</span>
      <input class="input" bind:value={sReason} required maxlength="500" placeholder="e.g. planned maintenance" />
    </label>
    <p class="muted small">Alerts still fire and are recorded; only notifications are suppressed.</p>
  </form>
  {#snippet footer()}
    <button class="btn" type="button" onclick={() => (silenceOpen = false)}>Cancel</button>
    <button class="btn primary" type="submit" form="silence-form" disabled={busy}>Create silence</button>
  {/snippet}
</Dialog>

<style>
  .summary span {
    display: block;
  }
  .summary {
    padding-top: 4px;
    padding-bottom: 4px;
  }
  .small {
    font-size: 11.5px;
  }
  .sans {
    font-family: var(--font-sans);
  }
  .ended td {
    color: var(--text-3);
  }
</style>
