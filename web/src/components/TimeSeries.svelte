<script lang="ts">
  import uPlot from 'uplot';
  import { onDestroy } from 'svelte';
  import { Table2, ChartLine } from '@lucide/svelte';
  import { theme } from '../lib/theme.svelte';
  import { alpha, chartColors } from '../lib/chart/palette';
  import { datetime, time as fmtTime, date as fmtDate } from '../lib/format';

  export interface ChartSeries {
    label: string;
    /** Categorical slot 1-8, fixed per entity. */
    slot: number;
    values: (number | null)[];
    fill?: boolean;
  }

  interface Props {
    title: string;
    subtitle?: string;
    xs: number[];
    series: ChartSeries[];
    format: (v: number) => string;
    yMin?: number;
    yMax?: number;
    syncKey?: string;
    height?: number;
    loading?: boolean;
    refreshing?: boolean;
    error?: string | null;
    bars?: boolean;
    empty?: string;
  }

  let {
    title,
    subtitle = '',
    xs,
    series,
    format,
    yMin = 0,
    yMax,
    syncKey,
    height = 160,
    loading = false,
    refreshing = false,
    error = null,
    bars = false,
    empty = 'No data for this range.',
  }: Props = $props();

  let host = $state<HTMLDivElement | undefined>();
  let tip = $state<HTMLDivElement | undefined>();
  let showTable = $state(false);
  let plot: uPlot | null = null;
  let observer: ResizeObserver | null = null;
  let builtFor = '';

  const hasData = $derived(xs.length > 0 && series.some((s) => s.values.some((v) => v !== null)));
  const lastValues = $derived(
    series.map((s) => {
      for (let i = s.values.length - 1; i >= 0; i--) {
        const v = s.values[i];
        if (v !== null && v !== undefined) return v;
      }
      return null;
    }),
  );

  function spanDays(): number {
    if (xs.length < 2) return 0;
    return ((xs[xs.length - 1] ?? 0) - (xs[0] ?? 0)) / 86400;
  }

  function buildTooltip(u: uPlot): void {
    const el = tip;
    if (!el) return;
    const idx = u.cursor.idx;
    if (idx === null || idx === undefined || u.cursor.left === undefined || u.cursor.left < 0) {
      el.hidden = true;
      return;
    }
    const ts = u.data[0]?.[idx];
    if (ts === undefined || ts === null) {
      el.hidden = true;
      return;
    }
    el.replaceChildren();
    const head = document.createElement('div');
    head.className = 'tt-time';
    head.textContent = datetime(ts);
    el.appendChild(head);
    const colors = chartColors();
    series.forEach((s, i) => {
      const raw = u.data[i + 1]?.[idx];
      const row = document.createElement('div');
      row.className = 'tt-row';
      const key = document.createElement('span');
      key.className = 'tt-key';
      key.style.setProperty('background', colors.series[(s.slot - 1) % 8] ?? colors.series[0] ?? '');
      const val = document.createElement('strong');
      val.textContent = raw === null || raw === undefined ? '–' : format(raw);
      const lab = document.createElement('span');
      lab.className = 'tt-label';
      lab.textContent = s.label;
      row.append(key, val, lab);
      el.appendChild(row);
    });
    el.hidden = false;
    const left = u.cursor.left ?? 0;
    const w = el.offsetWidth;
    const plotW = u.over.clientWidth;
    const x = left + 12 + w > plotW ? left - w - 12 : left + 12;
    el.style.setProperty('left', `${Math.max(0, x) + u.over.offsetLeft}px`);
    el.style.setProperty('top', `${u.over.offsetTop + 4}px`);
  }

  function build(): void {
    if (!host || !hasData) return;
    destroy();
    const c = chartColors();
    const width = Math.max(200, host.clientWidth);
    const days = spanDays();
    const axis = {
      stroke: c.label,
      grid: { stroke: c.grid, width: 1 },
      ticks: { stroke: c.axis, width: 1, size: 4 },
      font: '11px Inter Variable, system-ui, sans-serif',
    };
    const opts: uPlot.Options = {
      width,
      height,
      padding: [8, 8, 0, 0],
      legend: { show: false },
      cursor: {
        sync: syncKey ? { key: syncKey } : undefined,
        drag: { x: false, y: false },
        points: { size: 8, width: 2, stroke: c.surface },
      },
      scales: {
        x: { time: true },
        y: {
          range: (_u, dmin, dmax) => {
            const lo = Math.min(yMin, dmin ?? yMin);
            let hi = yMax ?? Math.max(dmax ?? 1, lo + 1e-9);
            if (yMax === undefined) hi = hi * 1.1;
            if (hi <= lo) hi = lo + 1;
            return [lo, hi];
          },
        },
      },
      axes: [
        {
          ...axis,
          values: (_u, ticks) => ticks.map((t) => (days > 2 ? fmtDate(t).slice(5) : fmtTime(t).slice(0, 5))),
          space: 64,
        },
        {
          ...axis,
          values: (_u, ticks) => ticks.map((t) => format(t)),
          size: 58,
          space: 28,
        },
      ],
      series: [
        {},
        ...series.map((s) => {
          const color = c.series[(s.slot - 1) % 8] ?? c.series[0] ?? '#2a78d6';
          const out: uPlot.Series = {
            label: s.label,
            stroke: color,
            width: bars ? 0 : 2,
            fill: bars ? color : s.fill ? alpha(color, 0.1) : undefined,
            points: { show: false },
            spanGaps: false,
          };
          if (bars) {
            out.paths = uPlot.paths.bars?.({ size: [0.7, 24], radius: 0.15 });
          }
          return out;
        }),
      ],
      hooks: {
        setCursor: [(u) => buildTooltip(u)],
      },
    };
    const data: uPlot.AlignedData = [xs, ...series.map((s) => s.values)];
    plot = new uPlot(opts, data, host);
    plot.over.addEventListener('mouseleave', () => {
      if (tip) tip.hidden = true;
    });
    builtFor = key();
    observer = new ResizeObserver(() => {
      if (plot && host) plot.setSize({ width: Math.max(200, host.clientWidth), height });
    });
    observer.observe(host);
  }

  function key(): string {
    return `${theme.version}|${series.map((s) => `${s.label}:${s.slot}`).join(',')}|${bars}|${height}|${yMax}`;
  }

  function destroy(): void {
    observer?.disconnect();
    observer = null;
    plot?.destroy();
    plot = null;
  }

  $effect(() => {
    // Track inputs.
    const k = key();
    const data: uPlot.AlignedData = [xs, ...series.map((s) => s.values)];
    if (!host || showTable) return;
    if (!hasData) {
      destroy();
      return;
    }
    if (!plot || k !== builtFor) build();
    else plot.setData(data);
  });

  onDestroy(destroy);

  const tableRows = $derived.by(() => {
    const rows: { ts: number; values: (number | null)[] }[] = [];
    for (let i = xs.length - 1; i >= 0 && rows.length < 1000; i--) {
      rows.push({ ts: xs[i] ?? 0, values: series.map((s) => s.values[i] ?? null) });
    }
    return rows;
  });
</script>

<section class="panel chart" aria-label={title}>
  <header class="panel-header">
    <h3>{title}</h3>
    {#if subtitle}<span class="muted sub">{subtitle}</span>{/if}
    <span class="spacer"></span>
    {#if series.length === 1 && lastValues[0] !== null && lastValues[0] !== undefined && !loading}
      <span class="last num">{format(lastValues[0])}</span>
    {/if}
    <button
      class="btn ghost icon small"
      type="button"
      aria-pressed={showTable}
      title={showTable ? 'Show chart' : 'Show data table'}
      onclick={() => (showTable = !showTable)}
    >
      {#if showTable}<ChartLine size={14} />{:else}<Table2 size={14} />{/if}
      <span class="sr-only">{showTable ? 'Show chart' : 'Show data table'}</span>
    </button>
  </header>
  {#if series.length > 1 && hasData && !showTable}
    <ul class="legend">
      {#each series as s, i (s.label)}
        <li>
          <span class={`key slot-${((s.slot - 1) % 8) + 1}`} class:bar={bars}></span>
          <span class="label">{s.label}</span>
          <span class="value num">{lastValues[i] === null || lastValues[i] === undefined ? '–' : format(lastValues[i] as number)}</span>
        </li>
      {/each}
    </ul>
  {/if}
  <div class="body" class:refreshing>
    {#if error}
      <p class="state error" role="alert">{error}</p>
    {:else if loading && !hasData}
      <div class="skeleton plot-skel" style:height={`${height}px`}></div>
    {:else if !hasData}
      <p class="state muted" style:height={`${height}px`}>{empty}</p>
    {:else if showTable}
      <div class="table-wrap table-view" style:max-height={`${height + 40}px`}>
        <table class="data">
          <thead>
            <tr>
              <th>Time</th>
              {#each series as s (s.label)}<th class="num">{s.label}</th>{/each}
            </tr>
          </thead>
          <tbody>
            {#each tableRows as r (r.ts)}
              <tr>
                <td class="mono nowrap">{datetime(r.ts)}</td>
                {#each r.values as v, i (i)}<td class="num">{v === null ? '–' : format(v)}</td>{/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <div class="plot" bind:this={host}></div>
      <div class="tooltip" bind:this={tip} hidden role="status"></div>
    {/if}
  </div>
</section>

<style>
  .chart {
    min-width: 0;
  }
  .sub {
    font-size: 11.5px;
  }
  .last {
    font-weight: 600;
    font-size: 12px;
  }
  .body {
    position: relative;
    padding: 6px 8px 4px 4px;
    transition: opacity var(--t-fast);
  }
  .body.refreshing {
    opacity: 0.5;
  }
  .plot {
    width: 100%;
    min-height: 40px;
  }
  .state {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 80px;
    font-size: 12px;
  }
  .state.error {
    color: var(--critical-text);
  }
  .plot-skel {
    margin: 4px;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 14px;
    list-style: none;
    margin: 0;
    padding: 6px 12px 0;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .legend li {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .legend .value {
    color: var(--text);
    font-weight: 500;
  }
  .key {
    display: inline-block;
    width: 12px;
    height: 2px;
    border-radius: 1px;
    background: var(--series-1);
  }
  .key.bar {
    height: 8px;
    width: 8px;
    border-radius: 2px;
  }
  .slot-1 {
    background: var(--series-1);
  }
  .slot-2 {
    background: var(--series-2);
  }
  .slot-3 {
    background: var(--series-3);
  }
  .slot-4 {
    background: var(--series-4);
  }
  .slot-5 {
    background: var(--series-5);
  }
  .slot-6 {
    background: var(--series-6);
  }
  .slot-7 {
    background: var(--series-7);
  }
  .slot-8 {
    background: var(--series-8);
  }
  .tooltip {
    position: absolute;
    z-index: 5;
    pointer-events: none;
    min-width: 140px;
    padding: 6px 8px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.12);
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
  }
  .tooltip :global(.tt-time) {
    color: var(--text-3);
    margin-bottom: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .tooltip :global(.tt-row) {
    display: flex;
    align-items: center;
    gap: 6px;
    line-height: 18px;
  }
  .tooltip :global(.tt-key) {
    width: 10px;
    height: 2px;
    border-radius: 1px;
    flex: none;
  }
  .tooltip :global(strong) {
    color: var(--text);
    font-weight: 600;
  }
  .tooltip :global(.tt-label) {
    color: var(--text-2);
  }
  .table-view {
    overflow: auto;
  }
</style>
