// Live fleet state: initial list from /hosts, then server-sent events.

import { api, ApiError } from './api';
import type { HostSummary, SparkPoint } from './types';

const SPARK_MAX = 180;

export const fleet = $state({
  hosts: {} as Record<string, HostSummary>,
  loaded: false,
  error: null as string | null,
  /** Live stream connected. */
  live: false,
  /** Incremented when alerts change, so pages refetch. */
  alertsVersion: 0,
  changesVersion: 0,
  hostKeysVersion: 0,
  lastEvent: 0,
});

let source: EventSource | null = null;

export async function loadFleet(): Promise<void> {
  try {
    const list = await api.get<HostSummary[]>('/hosts');
    const next: Record<string, HostSummary> = {};
    for (const h of list) next[h.name] = h;
    fleet.hosts = next;
    fleet.error = null;
  } catch (e) {
    fleet.error = e instanceof ApiError ? e.message : 'Cannot load hosts.';
  } finally {
    fleet.loaded = true;
  }
}

/** Merges a live summary, extending the locally kept sparkline. */
export function applyUpdate(s: HostSummary): void {
  const prev = fleet.hosts[s.name];
  let spark: SparkPoint[] = prev?.spark ? [...prev.spark] : [];
  const last = spark[spark.length - 1];
  if (s.last_success && (!last || last.ts < s.last_success) && s.connection === 'ok') {
    spark.push({ ts: s.last_success, cpu: s.cpu_pct, mem: s.mem_pct });
    if (spark.length > SPARK_MAX) spark = spark.slice(spark.length - SPARK_MAX);
  }
  fleet.hosts[s.name] = { ...s, spark };
}

export function connectFleet(): void {
  if (source) return;
  source = new EventSource('/api/v1/events');
  source.addEventListener('hello', () => {
    fleet.live = true;
    // Resynchronise after (re)connecting.
    void loadFleet();
    fleet.alertsVersion++;
  });
  source.addEventListener('resync', () => {
    void loadFleet();
    fleet.alertsVersion++;
  });
  source.addEventListener('host', (ev) => {
    try {
      const msg = JSON.parse((ev as MessageEvent<string>).data) as { data: HostSummary };
      applyUpdate(msg.data);
      fleet.lastEvent = Date.now();
    } catch {
      // ignore malformed events
    }
  });
  source.addEventListener('alerts', () => {
    fleet.alertsVersion++;
  });
  source.addEventListener('changes', () => {
    fleet.changesVersion++;
  });
  source.addEventListener('hostkeys', () => {
    fleet.hostKeysVersion++;
  });
  source.onerror = () => {
    fleet.live = false;
  };
}

export function disconnectFleet(): void {
  source?.close();
  source = null;
  fleet.live = false;
}

export function hostList(): HostSummary[] {
  return Object.values(fleet.hosts);
}
