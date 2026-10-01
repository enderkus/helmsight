<script lang="ts">
  import { Package, Network, Cog, Cpu, Monitor } from '@lucide/svelte';
  import { date, time } from '../lib/format';
  import type { Change } from '../lib/types';

  interface Props {
    changes: Change[];
    showHost?: boolean;
  }
  let { changes, showHost = false }: Props = $props();

  const days = $derived.by(() => {
    const out: { day: string; items: Change[] }[] = [];
    for (const c of changes) {
      const d = date(c.ts);
      const last = out[out.length - 1];
      if (last && last.day === d) last.items.push(c);
      else out.push({ day: d, items: [c] });
    }
    return out;
  });

  const kindLabel: Record<string, string> = {
    package: 'Package',
    port: 'Port',
    unit: 'Unit',
    kernel: 'Kernel',
    os: 'OS',
  };
</script>

<div class="timeline">
  {#each days as d (d.day)}
    <section>
      <h3 class="day">{d.day}</h3>
      <table class="data">
        <tbody>
          {#each d.items as c (c.id)}
            <tr>
              <td class="t mono muted">{time(c.ts).slice(0, 5)}</td>
              {#if showHost}<td class="h"><a class="mono" href={`/hosts/${encodeURIComponent(c.host)}?tab=changes`}>{c.host}</a></td>{/if}
              <td class="k">
                <span class="kind">
                  {#if c.kind === 'package'}<Package size={13} aria-hidden="true" />
                  {:else if c.kind === 'port'}<Network size={13} aria-hidden="true" />
                  {:else if c.kind === 'unit'}<Cog size={13} aria-hidden="true" />
                  {:else if c.kind === 'kernel'}<Cpu size={13} aria-hidden="true" />
                  {:else}<Monitor size={13} aria-hidden="true" />{/if}
                  {kindLabel[c.kind] ?? c.kind}
                </span>
              </td>
              <td class="a"><span class={`act ${c.action}`}>{c.action}</span></td>
              <td class="s mono">{c.subject}</td>
              <td class="v mono">
                {#if c.action === 'changed'}<span class="old">{c.old}</span> → <span>{c.new}</span>
                {:else if c.action === 'added'}{c.new}
                {:else}<span class="old">{c.old}</span>{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/each}
</div>

<style>
  .timeline {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .day {
    font-size: 12px;
    color: var(--text-2);
    padding: 8px 10px 4px;
    font-variant-numeric: tabular-nums;
  }
  .t {
    width: 56px;
  }
  .h {
    width: 160px;
  }
  .k {
    width: 100px;
  }
  .a {
    width: 90px;
  }
  .kind {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
  }
  .act {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
  }
  .act::before {
    content: '';
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .act.added::before {
    background: var(--series-3);
  }
  .act.removed::before {
    background: var(--series-2);
  }
  .act.changed::before {
    background: var(--series-1);
  }
  .s {
    font-weight: 500;
    overflow-wrap: anywhere;
  }
  .v {
    color: var(--text-2);
    overflow-wrap: anywhere;
    font-size: 11.5px;
  }
  .old {
    color: var(--text-3);
  }
</style>
