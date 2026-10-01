// Human formatting of values. Exact values go into `title` tooltips.

const BIN = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'];

export function bytes(v: number | null | undefined, digits = 1): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '–';
  let n = Math.abs(v);
  let i = 0;
  while (n >= 1024 && i < BIN.length - 1) {
    n /= 1024;
    i++;
  }
  const s = i === 0 ? String(Math.round(n)) : n.toFixed(n >= 100 ? 0 : digits);
  return `${v < 0 ? '-' : ''}${s} ${BIN[i]}`;
}

export function rate(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '–';
  return `${bytes(v)}/s`;
}

export function pct(v: number | null | undefined, digits = 1): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '–';
  return `${v.toFixed(v >= 99.95 || v === 0 ? 0 : digits)}%`;
}

export function num(v: number | null | undefined, digits = 2): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '–';
  return v.toFixed(digits);
}

export function count(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '–';
  return Math.round(v).toLocaleString('en-US');
}

/** Compact counts: 1,284 / 12.9K / 4.2M */
export function compact(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '–';
  const a = Math.abs(v);
  if (a < 10_000) return Math.round(v).toLocaleString('en-US');
  if (a < 1_000_000) return `${(v / 1000).toFixed(1)}K`;
  return `${(v / 1_000_000).toFixed(1)}M`;
}

/** Duration in seconds, e.g. `3d 4h`, `12m`, `45s`. */
export function duration(secs: number | null | undefined): string {
  if (secs === null || secs === undefined || !Number.isFinite(secs)) return '–';
  const s = Math.max(0, Math.floor(secs));
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d > 0) return h > 0 ? `${d}d ${h}h` : `${d}d`;
  if (h > 0) return m > 0 ? `${h}h ${m}m` : `${h}h`;
  if (m > 0) return `${m}m`;
  return `${s}s`;
}

/** Relative time: `just now`, `12s ago`, `5m ago`, `in 3d`. */
export function ago(ts: number | null | undefined, now: number): string {
  if (!ts) return 'never';
  const d = now - ts;
  if (d < 0) return `in ${duration(-d)}`;
  if (d < 3) return 'just now';
  return `${duration(d)} ago`;
}

const pad = (n: number) => String(n).padStart(2, '0');

/** Local date and time, `2026-09-30 17:40:12`. */
export function datetime(ts: number | null | undefined): string {
  if (!ts) return '–';
  const d = new Date(ts * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function date(ts: number | null | undefined): string {
  if (!ts) return '–';
  const d = new Date(ts * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function time(ts: number | null | undefined): string {
  if (!ts) return '–';
  const d = new Date(ts * 1000);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${count(n)} ${n === 1 ? one : many}`;
}
