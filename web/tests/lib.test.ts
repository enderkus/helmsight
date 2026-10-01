import { describe, expect, it, vi, beforeEach } from 'vitest';
import { bytes, rate, pct, duration, ago, compact, count } from '../src/lib/format';
import { statusInfo, statusRank, usageTone } from '../src/lib/status';
import { match } from '../src/lib/router.svelte';
import { align, instances, values } from '../src/lib/series';
import { api, ApiError, onAuthProblem, qs, setCsrf } from '../src/lib/api';
import { presetRange, rangeFromQuery } from '../src/lib/timerange';

describe('format', () => {
  it('formats bytes with binary units', () => {
    expect(bytes(0)).toBe('0 B');
    expect(bytes(1536)).toBe('1.5 KiB');
    expect(bytes(5 * 1024 ** 3)).toBe('5.0 GiB');
    expect(bytes(null)).toBe('–');
    expect(rate(2048)).toBe('2.0 KiB/s');
  });
  it('formats percentages, counts and durations', () => {
    expect(pct(43.27)).toBe('43.3%');
    expect(pct(100)).toBe('100%');
    expect(pct(undefined)).toBe('–');
    expect(count(12345)).toBe('12,345');
    expect(compact(12_900)).toBe('12.9K');
    expect(duration(90061)).toBe('1d 1h');
    expect(duration(59)).toBe('59s');
    expect(ago(100, 100)).toBe('just now');
    expect(ago(100, 400)).toBe('5m ago');
    expect(ago(null, 1)).toBe('never');
  });
});

describe('status', () => {
  it('maps every host state to a label and a tone', () => {
    expect(statusInfo('host_key_changed')).toMatchObject({ label: 'Key changed', tone: 'critical' });
    expect(statusInfo('unreachable').label).toBe('Down');
    expect(statusInfo('mystery').tone).toBe('unknown');
  });
  it('sorts the most urgent first', () => {
    const s = ['ok', 'warning', 'host_key_changed', 'critical', 'pending'].sort((a, b) => statusRank(a) - statusRank(b));
    expect(s).toEqual(['host_key_changed', 'critical', 'warning', 'pending', 'ok']);
  });
  it('assigns usage tones by threshold', () => {
    expect(usageTone(50)).toBe('ok');
    expect(usageTone(85)).toBe('warning');
    expect(usageTone(95)).toBe('critical');
    expect(usageTone(null)).toBe('unknown');
  });
});

describe('router', () => {
  it('matches patterns and decodes parameters', () => {
    expect(match('/hosts/:name', '/hosts/web%201')).toEqual({ name: 'web 1' });
    expect(match('/hosts/:name', '/hosts')).toBeNull();
    expect(match('/', '/')).toEqual({});
    expect(match('/hosts/:name', '/hosts/%E0%A4%A')).toBeNull();
  });
});

describe('series', () => {
  it('aligns series on the union of timestamps', () => {
    const a = align({
      resolution: 'raw',
      series: [
        { key: 'cpu.busy', points: [[10, 1, 1, 1], [20, 2, 2, 2]] },
        { key: 'fs.used_pct:/var', points: [[20, 50, 50, 50], [30, 51, 51, 51]] },
        { key: 'fs.used_pct:/', points: [[30, 9, 9, 9]] },
      ],
    });
    expect(a.xs).toEqual([10, 20, 30]);
    expect(values(a, 'cpu.busy')).toEqual([1, 2, null]);
    expect(values(a, 'missing')).toEqual([null, null, null]);
    expect(instances(a, 'fs.used_pct:')).toEqual(['/', '/var']);
  });
});

describe('time ranges', () => {
  it('parses presets and custom ranges', () => {
    expect(presetRange('6h', 100_000)).toEqual({ key: '6h', from: 100_000 - 21_600, to: 100_000, live: true });
    expect(rangeFromQuery(new URLSearchParams('from=10&to=20'))).toMatchObject({ key: 'custom', live: false });
    expect(rangeFromQuery(new URLSearchParams('range=bogus'), 5000).key).toBe('1h');
  });
});

describe('api client', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('sends the CSRF header on state-changing requests only', async () => {
    const fetchMock = vi.fn(async () => new Response('{"ok":true}', { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);
    setCsrf('tok');
    await api.get('/hosts');
    await api.post('/silences', { a: 1 });
    const getHeaders = (fetchMock.mock.calls[0] as unknown as [string, RequestInit])[1].headers as Record<string, string>;
    const postHeaders = (fetchMock.mock.calls[1] as unknown as [string, RequestInit])[1].headers as Record<string, string>;
    expect(getHeaders['X-CSRF-Token']).toBeUndefined();
    expect(postHeaders['X-CSRF-Token']).toBe('tok');
    expect((fetchMock.mock.calls[0] as unknown as [string])[0]).toBe('/api/v1/hosts');
  });

  it('turns error bodies into ApiError and notifies on 401', async () => {
    const seen: string[] = [];
    onAuthProblem((e) => seen.push(e.code));
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{"error":"unauthorized","message":"Sign in again."}', { status: 401 })));
    await expect(api.get('/hosts')).rejects.toMatchObject({ status: 401, code: 'unauthorized', message: 'Sign in again.' });
    expect(seen).toContain('unauthorized');
  });

  it('reports network failures with an actionable message', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => {
      throw new TypeError('Failed to fetch');
    }));
    const err = await api.get('/meta').catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).code).toBe('network');
  });

  it('builds query strings without empty values', () => {
    expect(qs({ a: 'x y', b: null, c: '', d: 3 })).toBe('?a=x+y&d=3');
    expect(qs({})).toBe('');
  });
});
