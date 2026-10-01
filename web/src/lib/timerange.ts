// Time ranges for history charts.

export interface Range {
  /** Preset key or `custom`. */
  key: string;
  from: number;
  to: number;
  /** Relative ranges follow "now". */
  live: boolean;
}

export const PRESETS: { key: string; label: string; secs: number }[] = [
  { key: '15m', label: 'Last 15 minutes', secs: 900 },
  { key: '1h', label: 'Last hour', secs: 3600 },
  { key: '6h', label: 'Last 6 hours', secs: 6 * 3600 },
  { key: '24h', label: 'Last 24 hours', secs: 86400 },
  { key: '7d', label: 'Last 7 days', secs: 7 * 86400 },
  { key: '30d', label: 'Last 30 days', secs: 30 * 86400 },
];

export function presetRange(key: string, now = Math.floor(Date.now() / 1000)): Range {
  const p = PRESETS.find((x) => x.key === key) ?? PRESETS[1]!;
  return { key: p.key, from: now - p.secs, to: now, live: true };
}

/** Parses `?range=1h` or `?from=..&to=..` from a query string. */
export function rangeFromQuery(q: URLSearchParams, now = Math.floor(Date.now() / 1000)): Range {
  const from = Number(q.get('from'));
  const to = Number(q.get('to'));
  if (Number.isFinite(from) && Number.isFinite(to) && from > 0 && to > from) {
    return { key: 'custom', from, to, live: false };
  }
  return presetRange(q.get('range') ?? '1h', now);
}

/** Converts a `datetime-local` input value to unix seconds. */
export function fromLocalInput(v: string): number | null {
  const t = new Date(v).getTime();
  return Number.isFinite(t) ? Math.floor(t / 1000) : null;
}

export function toLocalInput(ts: number): string {
  const d = new Date(ts * 1000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
