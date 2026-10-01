<script lang="ts">
  interface Props {
    values: (number | null)[];
    /** Fixed scale maximum; percentages use 100. */
    max?: number;
    width?: number;
    height?: number;
    label: string;
  }
  let { values, max = 100, width = 72, height = 20, label }: Props = $props();

  const pts = $derived.by(() => {
    const clean = values.map((v) => (v === null || !Number.isFinite(v) ? null : v));
    const n = clean.length;
    if (n < 2) return [] as [number, number][];
    const m = Math.max(max, ...clean.filter((v): v is number => v !== null));
    const out: [number, number][] = [];
    clean.forEach((v, i) => {
      if (v === null) return;
      const x = (i / (n - 1)) * (width - 3) + 1.5;
      const y = height - 1.5 - (Math.max(0, v) / (m || 1)) * (height - 3);
      out.push([x, y]);
    });
    return out;
  });
  const line = $derived(pts.map(([x, y], i) => `${i ? 'L' : 'M'}${x.toFixed(1)},${y.toFixed(1)}`).join(''));
  const area = $derived(
    pts.length ? `${line}L${pts[pts.length - 1]![0].toFixed(1)},${height}L${pts[0]![0].toFixed(1)},${height}Z` : '',
  );
  const end = $derived(pts[pts.length - 1]);
</script>

<svg class="spark" {width} {height} viewBox={`0 0 ${width} ${height}`} role="img" aria-label={label}>
  {#if pts.length >= 2}
    <path class="area" d={area} />
    <path class="line" d={line} />
    {#if end}<circle class="end" cx={end[0]} cy={end[1]} r="2" />{/if}
  {:else}
    <line class="none" x1="0" x2={width} y1={height - 1} y2={height - 1} />
  {/if}
</svg>

<style>
  .spark {
    display: block;
    overflow: visible;
  }
  .line {
    fill: none;
    stroke: var(--series-1);
    stroke-width: 1.5;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .area {
    fill: var(--series-1);
    opacity: 0.1;
  }
  .end {
    fill: var(--series-1);
    stroke: var(--surface);
    stroke-width: 1.5;
  }
  .none {
    stroke: var(--border);
    stroke-width: 1;
  }
</style>
