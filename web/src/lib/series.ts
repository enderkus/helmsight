import type { QueryResult } from './types';

export interface Aligned {
  xs: number[];
  /** key -> values aligned with xs (average of each point). */
  byKey: Map<string, (number | null)[]>;
  resolution: string;
}

/** Aligns all series of a query on the union of their timestamps. */
export function align(r: QueryResult | null): Aligned {
  if (!r) return { xs: [], byKey: new Map(), resolution: 'raw' };
  const set = new Set<number>();
  for (const s of r.series) for (const p of s.points) set.add(p[0]);
  const sorted = [...set].sort((a, b) => a - b);
  // Break lines where no samples exist (host down, server stopped) instead
  // of drawing a straight line across the gap.
  const step = medianStep(sorted);
  const xs: number[] = [];
  for (const [i, x] of sorted.entries()) {
    const prev = sorted[i - 1];
    if (prev !== undefined && step > 0 && x - prev > step * 3) xs.push(prev + step);
    xs.push(x);
  }
  const index = new Map(xs.map((x, i) => [x, i]));
  const byKey = new Map<string, (number | null)[]>();
  for (const s of r.series) {
    const vals: (number | null)[] = new Array(xs.length).fill(null);
    for (const [ts, avg] of s.points) {
      const i = index.get(ts);
      if (i !== undefined) vals[i] = avg;
    }
    byKey.set(s.key, vals);
  }
  return { xs, byKey, resolution: r.resolution };
}

function medianStep(xs: number[]): number {
  if (xs.length < 3) return 0;
  const d = xs.slice(1).map((x, i) => x - (xs[i] as number)).sort((a, b) => a - b);
  return d[Math.floor(d.length / 2)] ?? 0;
}

/** Keys with a prefix, e.g. `fs.used_pct:` -> mount names, sorted. */
export function instances(a: Aligned, prefix: string): string[] {
  return [...a.byKey.keys()]
    .filter((k) => k.startsWith(prefix))
    .map((k) => k.slice(prefix.length))
    .sort((x, y) => x.localeCompare(y, undefined, { numeric: true }));
}

export function values(a: Aligned, key: string): (number | null)[] {
  return a.byKey.get(key) ?? new Array(a.xs.length).fill(null);
}
