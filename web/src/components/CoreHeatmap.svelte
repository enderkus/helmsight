<script lang="ts">
  import { onDestroy } from 'svelte';
  import { Table2, Grid3x3 } from '@lucide/svelte';
  import { theme } from '../lib/theme.svelte';
  import { datetime, pct } from '../lib/format';

  interface Props {
    /** Timestamps (columns). */
    xs: number[];
    /** One row of busy percentages per core. */
    rows: (number | null)[][];
    loading?: boolean;
    refreshing?: boolean;
    height?: number;
  }
  let { xs, rows, loading = false, refreshing = false, height = 160 }: Props = $props();

  let canvas = $state<HTMLCanvasElement | undefined>();
  let wrap = $state<HTMLDivElement | undefined>();
  let readout = $state('');
  let showTable = $state(false);
  let width = $state(600);
  let observer: ResizeObserver | null = null;

  // Sequential single-hue ramp (blue), light to dark; 0% recedes toward the surface.
  const LIGHT = ['#eef4fc', '#cde2fb', '#9ec5f4', '#6da7ec', '#3987e5', '#256abf', '#184f95', '#0d366b'];
  const DARK = ['#1f2530', '#1c3150', '#184f95', '#1c5cab', '#2a78d6', '#5598e7', '#86b6ef', '#cde2fb'];

  function color(v: number | null): string {
    const ramp = theme.dark ? DARK : LIGHT;
    if (v === null) return theme.dark ? '#222220' : '#f3f3f0';
    const i = Math.min(ramp.length - 1, Math.floor((Math.max(0, Math.min(100, v)) / 100) * ramp.length));
    return ramp[i] ?? ramp[0]!;
  }

  function draw() {
    const c = canvas;
    if (!c || !xs.length || !rows.length) return;
    const dpr = window.devicePixelRatio || 1;
    const w = width;
    const h = height;
    c.width = Math.floor(w * dpr);
    c.height = Math.floor(h * dpr);
    c.style.setProperty('width', `${w}px`);
    c.style.setProperty('height', `${h}px`);
    const ctx = c.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    const cw = w / xs.length;
    const rh = h / rows.length;
    const gap = rows.length <= 32 && rh > 4 ? 1 : 0;
    rows.forEach((row, r) => {
      row.forEach((v, i) => {
        ctx.fillStyle = color(v);
        ctx.fillRect(i * cw, r * rh, Math.ceil(cw), Math.max(1, rh - gap));
      });
    });
  }

  $effect(() => {
    void theme.version;
    void xs;
    void rows;
    void width;
    if (!showTable) draw();
  });

  $effect(() => {
    if (!wrap) return;
    observer?.disconnect();
    observer = new ResizeObserver(() => {
      if (wrap) width = Math.max(200, wrap.clientWidth);
    });
    observer.observe(wrap);
    width = Math.max(200, wrap.clientWidth);
  });

  onDestroy(() => observer?.disconnect());

  function move(e: PointerEvent) {
    if (!canvas || !xs.length || !rows.length) return;
    const rect = canvas.getBoundingClientRect();
    const i = Math.min(xs.length - 1, Math.max(0, Math.floor(((e.clientX - rect.left) / rect.width) * xs.length)));
    const r = Math.min(rows.length - 1, Math.max(0, Math.floor(((e.clientY - rect.top) / rect.height) * rows.length)));
    const v = rows[r]?.[i] ?? null;
    readout = `core ${r} · ${datetime(xs[i])} · ${v === null ? 'no data' : pct(v)}`;
  }

  const latest = $derived(rows.map((row) => {
    for (let i = row.length - 1; i >= 0; i--) if (row[i] !== null && row[i] !== undefined) return row[i] as number;
    return null;
  }));
</script>

<section class="panel" aria-label="CPU per core">
  <header class="panel-header">
    <h3>CPU per core</h3>
    <span class="muted sub">busy %, {rows.length} cores</span>
    <span class="spacer"></span>
    <span class="scale" aria-hidden="true">
      0%<span class="ramp" class:dark={theme.dark}></span>100%
    </span>
    <button class="btn ghost icon small" type="button" aria-pressed={showTable} onclick={() => (showTable = !showTable)} title={showTable ? 'Show heatmap' : 'Show latest values'}>
      {#if showTable}<Grid3x3 size={14} />{:else}<Table2 size={14} />{/if}
      <span class="sr-only">{showTable ? 'Show heatmap' : 'Show latest values'}</span>
    </button>
  </header>
  <div class="body" class:refreshing>
    {#if loading && !xs.length}
      <div class="skeleton" style:height={`${height}px`}></div>
    {:else if !xs.length || !rows.length}
      <p class="empty muted" style:height={`${height}px`}>No per-core data for this range.</p>
    {:else if showTable}
      <div class="cores">
        {#each latest as v, i (i)}
          <div class="core"><span class="muted">core {i}</span><span class="num">{pct(v)}</span></div>
        {/each}
      </div>
    {:else}
      <div class="map" bind:this={wrap}>
        <canvas bind:this={canvas} onpointermove={move} onpointerleave={() => (readout = '')} aria-label="Per-core CPU heatmap; use the table view for values"></canvas>
      </div>
      <p class="readout muted">{readout || 'Rows are cores (top: core 0), columns are time. Hover for values.'}</p>
    {/if}
  </div>
</section>

<style>
  .sub {
    font-size: 11.5px;
  }
  .body {
    padding: 8px 12px;
    transition: opacity var(--t-fast);
  }
  .refreshing {
    opacity: 0.5;
  }
  .map {
    width: 100%;
  }
  canvas {
    display: block;
    border-radius: 2px;
  }
  .readout {
    margin-top: 6px;
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
    min-height: 17px;
  }
  .empty {
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
  }
  .scale {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-3);
  }
  .ramp {
    width: 64px;
    height: 8px;
    border-radius: 2px;
    background: linear-gradient(90deg, #eef4fc, #9ec5f4, #3987e5, #184f95, #0d366b);
  }
  .ramp.dark {
    background: linear-gradient(90deg, #1f2530, #184f95, #2a78d6, #86b6ef, #cde2fb);
  }
  .cores {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
    gap: 4px 16px;
    font-size: 12px;
  }
  .core {
    display: flex;
    justify-content: space-between;
    border-bottom: 1px solid var(--border);
    padding: 3px 0;
  }
</style>
