<script lang="ts">
  import { KeyRound, ShieldAlert } from '@lucide/svelte';
  import Dialog from '../../components/Dialog.svelte';
  import EmptyState from '../../components/EmptyState.svelte';
  import ErrorState from '../../components/ErrorState.svelte';
  import SkeletonRows from '../../components/SkeletonRows.svelte';
  import { api, ApiError } from '../../lib/api';
  import { fleet } from '../../lib/fleet.svelte';
  import { toast } from '../../lib/toast.svelte';
  import { datetime } from '../../lib/format';
  import type { PendingKey } from '../../lib/types';

  interface Props {
    params?: Record<string, string>;
  }
  let { params: _params = {} }: Props = $props();

  let keys = $state<PendingKey[] | null>(null);
  let error = $state<string | null>(null);
  let reviewing = $state<PendingKey | null>(null);
  let typed = $state('');
  let busy = $state(false);

  async function load() {
    try {
      keys = await api.get<PendingKey[]>('/hostkeys');
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Cannot load host keys.';
    }
  }
  $effect(() => {
    void fleet.hostKeysVersion;
    void load();
  });

  const normalized = (s: string) => s.trim().replace(/^SHA256:/, '');
  const matches = $derived(!!reviewing && normalized(typed) === normalized(reviewing.fingerprint));

  async function approve() {
    if (!reviewing) return;
    busy = true;
    try {
      for (const h of reviewing.hosts.slice(0, 1)) {
        await api.post('/hostkeys/approve', { host: h, fingerprint: reviewing.fingerprint });
      }
      toast('Host key trusted. Collection resumes on the next attempt.');
      reviewing = null;
      typed = '';
      await load();
    } catch (err) {
      toast(err instanceof ApiError ? err.message : 'Could not approve the key.', 'error');
    } finally {
      busy = false;
    }
  }

  async function reject(k: PendingKey) {
    const host = k.hosts[0];
    if (!host) return;
    try {
      await api.post('/hostkeys/reject', { host, fingerprint: k.fingerprint });
      toast('Key discarded. It will reappear if the host presents it again.');
      await load();
    } catch (err) {
      toast(err instanceof ApiError ? err.message : 'Could not reject the key.', 'error');
    }
  }
</script>

<div class="page">
  <div class="page-header">
    <h1>Host keys</h1>
    <span class="muted">SSH host keys waiting for a decision. Unknown and changed keys are never trusted automatically.</span>
  </div>
  {#if error}<ErrorState message={error} onretry={load} />
  {:else if !keys}<div class="panel"><SkeletonRows /></div>
  {:else if keys.length === 0}
    <div class="panel"><EmptyState title="No pending host keys">All hosts present trusted keys.</EmptyState></div>
  {:else}
    <div class="stack">
      {#each keys as k (k.endpoint)}
        <section class="panel key" class:changed={!!k.previous}>
          <header class="panel-header">
            {#if k.previous}<ShieldAlert size={16} aria-hidden="true" />{:else}<KeyRound size={16} aria-hidden="true" />{/if}
            <h2>{k.previous ? 'Changed key' : 'New key'} · <span class="mono">{k.hosts.join(', ') || k.endpoint}</span></h2>
            <span class="spacer"></span>
            <span class="muted small">first seen {datetime(k.seen_at)}</span>
          </header>
          <div class="panel-body stack">
            {#if k.previous}
              <p class="banner critical">
                The key differs from the trusted one. This happens after a reinstall, but can also mean someone is intercepting the connection. Verify with the host's owner before approving.
              </p>
            {/if}
            <dl class="props">
              <dt>Endpoint</dt><dd class="mono">{k.endpoint}</dd>
              <dt>Algorithm</dt><dd class="mono">{k.algorithm}</dd>
              <dt>Fingerprint</dt><dd class="mono fp">{k.fingerprint}</dd>
              {#if k.previous}<dt>Previously trusted</dt><dd class="mono fp">{k.previous}</dd>{/if}
            </dl>
            <div class="row">
              <button class="btn primary" type="button" onclick={() => ((reviewing = k), (typed = ''))}>Review and trust…</button>
              <button class="btn" type="button" onclick={() => reject(k)}>Discard</button>
            </div>
          </div>
        </section>
      {/each}
    </div>
  {/if}
</div>

<Dialog open={!!reviewing} title="Verify the fingerprint" onclose={() => (reviewing = null)} width={560}>
  {#if reviewing}
    <p>On the host, run:</p>
    <code class="block">ssh-keygen -lf /etc/ssh/ssh_host_{reviewing.algorithm.replace('ssh-', '').replace(/-.*/, '')}_key.pub</code>
    <p>and paste the <strong>SHA256</strong> fingerprint it prints. It must equal:</p>
    <code class="block">{reviewing.fingerprint}</code>
    <label class="field">
      <span>Fingerprint from the host</span>
      <input class="input mono" bind:value={typed} placeholder="SHA256:…" autocomplete="off" spellcheck="false" />
      {#if typed && !matches}<span class="hint mismatch">Does not match. Do not trust this key.</span>{/if}
    </label>
  {/if}
  {#snippet footer()}
    <button class="btn" type="button" onclick={() => (reviewing = null)}>Cancel</button>
    <button class="btn primary" type="button" disabled={!matches || busy} onclick={approve}>Trust key</button>
  {/snippet}
</Dialog>

<style>
  .key {
    border-left: 3px solid var(--warning);
  }
  .key.changed {
    border-left-color: var(--critical);
  }
  .key :global(.panel-header svg) {
    color: var(--warning);
  }
  .key.changed :global(.panel-header svg) {
    color: var(--critical);
  }
  .fp {
    font-size: 12.5px;
  }
  .small {
    font-size: 12px;
  }
  .block {
    display: block;
    padding: 6px 8px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow-wrap: anywhere;
  }
  .mismatch {
    color: var(--critical-text) !important;
  }
</style>
