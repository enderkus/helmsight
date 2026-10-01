<script lang="ts">
  import TimeSeries from '../../components/TimeSeries.svelte';
  import TimeRangePicker from '../../components/TimeRangePicker.svelte';
  import CoreHeatmap from '../../components/CoreHeatmap.svelte';
  import Meter from '../../components/Meter.svelte';
  import { api, ApiError, qs, seg } from '../../lib/api';
  import { router } from '../../lib/router.svelte';
  import { rangeFromQuery, type Range } from '../../lib/timerange';
  import { align, instances, values, type Aligned } from '../../lib/series';
  import { bytes, count, duration, num, pct, rate } from '../../lib/format';
  import type { HostDetail, HostSummary, QueryResult } from '../../lib/types';

  interface Props {
    host: string;
    detail: HostDetail | null;
    summary: HostSummary;
  }
  let { host, detail, summary }: Props = $props();

  let range = $state<Range>(rangeFromQuery(router.query));
  let data = $state<Aligned>(align(null));
  let loading = $state(true);
  let refreshing = $state(false);
  let error = $state<string | null>(null);
  let lastFetch = 0;
  let fetchKey = '';
  let disk = $state('');
  let iface = $state('');

  const SERIES = 'cpu.*,mem.*,swap.*,load.*,fs.used_pct:*,disk.*,net.*,tcp.*';

  function setRange(r: Range) {
    range = r;
    router.setQuery(r.key === 'custom' ? { range: null, from: String(r.from), to: String(r.to) } : { range: r.key === '1h' ? null : r.key, from: null, to: null });
  }

  async function fetchData(force: boolean) {
    const span = range.to - range.from;
    const now = Math.floor(Date.now() / 1000);
    const key = `${host}|${range.key}|${range.live ? '' : `${range.from}-${range.to}`}`;
    const minGap = span > 6 * 3600 ? 60_000 : 4_000;
    if (!force && key === fetchKey && Date.now() - lastFetch < minGap) return;
    const from = range.live ? now - span : range.from;
    const to = range.live ? now : range.to;
    if (key !== fetchKey) loading = true;
    else refreshing = true;
    fetchKey = key;
    lastFetch = Date.now();
    try {
      const r = await api.get<QueryResult>(`/hosts/${seg(host)}/metrics${qs({ series: SERIES, from, to, points: 720 })}`);
      data = align(r);
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Cannot load history.';
    } finally {
      loading = false;
      refreshing = false;
    }
  }

  // Refetch when the range changes, and on new samples for live ranges.
  $effect(() => {
    void range;
    void host;
    void fetchData(true);
  });
  $effect(() => {
    void summary.last_success;
    if (range.live) void fetchData(false);
  });

  const mounts = $derived(instances(data, 'fs.used_pct:'));
  const disks = $derived(instances(data, 'disk.read:'));
  const ifaces = $derived(instances(data, 'net.rx:'));
  const cores = $derived(instances(data, 'cpu.core:'));
  const diskSel = $derived(disks.includes(disk) ? disk : (disks[0] ?? ''));
  const ifaceSel = $derived(ifaces.includes(iface) ? iface : (ifaces[0] ?? ''));

  const m = $derived(detail?.metrics?.data ?? null);
  const sync = $derived(`host:${host}`);
  const common = $derived({ xs: data.xs, syncKey: sync, loading, refreshing, error });
  const f = { pct: (v: number) => pct(v), bytes: (v: number) => bytes(v), rate: (v: number) => rate(v), num: (v: number) => num(v), count: (v: number) => count(v) };
  const memTotal = $derived.by(() => {
    const t = values(data, 'mem.total').filter((v): v is number => v !== null);
    return t.length ? Math.max(...t) : undefined;
  });
</script>

<div class="stack">
  <div class="tiles">
    <div class="tile panel">
      <span class="label">CPU</span>
      <span class="value">{pct(summary.cpu_pct)}</span>
      <span class="hint">{m?.cpu ? `${m.cpu.cores} cores · iowait ${pct(m.cpu.total.iowait)} · steal ${pct(m.cpu.total.steal)}` : '–'}</span>
    </div>
    <div class="tile panel">
      <span class="label">Memory</span>
      <span class="value">{pct(summary.mem_pct)}</span>
      <span class="hint">{m?.mem ? `${bytes(m.mem.used)} of ${bytes(m.mem.total)} · swap ${m.mem.swap_total ? pct(m.mem.swap_used_pct) : 'none'}` : '–'}</span>
    </div>
    <div class="tile panel">
      <span class="label">Load average</span>
      <span class="value">{num(summary.load1)}</span>
      <span class="hint">5m {num(summary.load5)} · 15m {num(summary.load15)}{summary.cores ? ` · ${summary.cores} cores` : ''}</span>
    </div>
    <div class="tile panel">
      <span class="label">Fullest disk</span>
      <span class="value">{summary.disk ? pct(summary.disk.used_pct) : '–'}</span>
      <span class="hint mono truncate">{summary.disk ? `${summary.disk.mount} · ${bytes(summary.disk.avail)} free` : '–'}</span>
    </div>
    <div class="tile panel">
      <span class="label">Processes</span>
      <span class="value">{count(m?.procs?.total)}</span>
      <span class="hint">{m?.procs ? `${m.procs.running} running · ${m.procs.blocked} blocked` : '–'}</span>
    </div>
    <div class="tile panel">
      <span class="label">TCP connections</span>
      <span class="value">{count(m?.tcp?.established)}</span>
      <span class="hint">{m?.tcp ? `established · ${count(m.tcp.time_wait)} time-wait · ${count(m.tcp.listen)} listening` : '–'}</span>
    </div>
    <div class="tile panel">
      <span class="label">Uptime</span>
      <span class="value">{duration(summary.uptime_secs)}</span>
      <span class="hint">{summary.reboot_required ? 'Reboot required' : summary.reboot_required === false ? 'No reboot required' : 'Reboot status unknown'}</span>
    </div>
  </div>

  <div class="range-row">
    <TimeRangePicker value={range} onchange={setRange} />
    <span class="muted res">Resolution: {data.resolution === 'raw' ? 'every sample' : `${data.resolution} averages`}</span>
  </div>

  <div class="charts">
    <TimeSeries
      title="CPU"
      subtitle="% of all cores"
      {...common}
      yMax={100}
      format={f.pct}
      series={[
        { label: 'user', slot: 1, values: values(data, 'cpu.user'), fill: true },
        { label: 'system', slot: 2, values: values(data, 'cpu.system') },
        { label: 'iowait', slot: 3, values: values(data, 'cpu.iowait') },
        { label: 'steal', slot: 4, values: values(data, 'cpu.steal') },
      ]}
    />
    <CoreHeatmap xs={data.xs} rows={cores.map((c) => values(data, `cpu.core:${c}`))} {loading} {refreshing} />
    <TimeSeries
      title="Memory"
      subtitle="bytes"
      {...common}
      yMax={memTotal}
      format={f.bytes}
      series={[
        { label: 'used', slot: 1, values: values(data, 'mem.used'), fill: true },
        { label: 'cache and buffers', slot: 2, values: values(data, 'mem.cached') },
        { label: 'swap used', slot: 3, values: values(data, 'swap.used') },
      ]}
    />
    <TimeSeries
      title="Load average"
      subtitle={summary.cores ? `${summary.cores} cores` : ''}
      {...common}
      format={f.num}
      series={[
        { label: '1 min', slot: 1, values: values(data, 'load.1') },
        { label: '5 min', slot: 2, values: values(data, 'load.5') },
        { label: '15 min', slot: 3, values: values(data, 'load.15') },
      ]}
    />
    <TimeSeries
      title="Disk usage"
      subtitle={mounts.length > 8 ? `% used · first 8 of ${mounts.length} mounts` : '% used per mount'}
      {...common}
      yMax={100}
      format={f.pct}
      series={mounts.slice(0, 8).map((mnt, i) => ({ label: mnt, slot: i + 1, values: values(data, `fs.used_pct:${mnt}`) }))}
    />
    <div class="with-select">
      {#if disks.length > 1}
        <label class="picker"><span class="sr-only">Device</span>
          <select class="select" bind:value={disk}>{#each disks as d (d)}<option value={d}>{d}</option>{/each}</select>
        </label>
      {/if}
      <TimeSeries
        title={`Disk I/O${diskSel ? ` · ${diskSel}` : ''}`}
        subtitle="bytes per second"
        {...common}
        format={f.rate}
        empty="No block device activity recorded."
        series={diskSel
          ? [
              { label: 'read', slot: 1, values: values(data, `disk.read:${diskSel}`), fill: true },
              { label: 'write', slot: 2, values: values(data, `disk.write:${diskSel}`) },
            ]
          : []}
      />
    </div>
    <div class="with-select">
      {#if ifaces.length > 1}
        <label class="picker"><span class="sr-only">Interface</span>
          <select class="select" bind:value={iface}>{#each ifaces as i (i)}<option value={i}>{i}</option>{/each}</select>
        </label>
      {/if}
      <TimeSeries
        title={`Network${ifaceSel ? ` · ${ifaceSel}` : ''}`}
        subtitle="bytes per second"
        {...common}
        format={f.rate}
        empty="No network interfaces recorded."
        series={ifaceSel
          ? [
              { label: 'received', slot: 1, values: values(data, `net.rx:${ifaceSel}`), fill: true },
              { label: 'sent', slot: 2, values: values(data, `net.tx:${ifaceSel}`) },
            ]
          : []}
      />
    </div>
    <TimeSeries
      title="TCP connections"
      {...common}
      format={f.count}
      series={[
        { label: 'established', slot: 1, values: values(data, 'tcp.established') },
        { label: 'time-wait', slot: 2, values: values(data, 'tcp.time_wait') },
      ]}
    />
  </div>

  {#if m && m.filesystems.length}
    <section class="panel">
      <header class="panel-header"><h2>Filesystems</h2><span class="muted">now</span></header>
      <div class="table-wrap">
        <table class="data">
          <thead>
            <tr><th>Mount</th><th>Device</th><th>Type</th><th class="num">Size</th><th class="num">Used</th><th class="num">Free</th><th>Usage</th><th class="num">Inodes</th></tr>
          </thead>
          <tbody>
            {#each m.filesystems as fs (fs.mount)}
              <tr>
                <td class="mono">{fs.mount}</td>
                <td class="mono muted">{fs.device}</td>
                <td class="muted">{fs.fstype ?? '–'}</td>
                <td class="num" title={`${fs.total} bytes`}>{bytes(fs.total)}</td>
                <td class="num" title={`${fs.used} bytes`}>{bytes(fs.used)}</td>
                <td class="num" title={`${fs.avail} bytes`}>{bytes(fs.avail)}</td>
                <td><Meter value={fs.used_pct} label={`Space used on ${fs.mount}`} width={100} /></td>
                <td class="num">{fs.inodes_used_pct === null ? '–' : pct(fs.inodes_used_pct)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {/if}
</div>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(148px, 1fr));
    gap: 8px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 12px;
    min-width: 0;
  }
  .tile .label {
    font-size: 12px;
    color: var(--text-2);
  }
  .tile .value {
    font-size: 20px;
    font-weight: 600;
    line-height: 1.25;
  }
  .tile .hint {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .range-row {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .res {
    font-size: 12px;
  }
  .charts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(440px, 1fr));
    gap: 10px;
  }
  .with-select {
    position: relative;
    min-width: 0;
  }
  .picker {
    position: absolute;
    right: 44px;
    top: 5px;
    z-index: 2;
  }
  .picker .select {
    height: 26px;
    font-size: 12px;
  }
  @media (max-width: 520px) {
    .charts {
      grid-template-columns: 1fr;
    }
  }
</style>
