import type { HostStatus } from './types';

export type Tone = 'ok' | 'warning' | 'critical' | 'unknown';

export interface StatusInfo {
  label: string;
  tone: Tone;
  /** One-line explanation for tooltips and banners. */
  hint: string;
}

const MAP: Record<HostStatus, StatusInfo> = {
  ok: { label: 'OK', tone: 'ok', hint: 'Collecting normally, no alerts firing.' },
  warning: { label: 'Warning', tone: 'warning', hint: 'Warning alerts are firing.' },
  critical: { label: 'Critical', tone: 'critical', hint: 'Critical alerts are firing.' },
  pending: { label: 'Pending', tone: 'unknown', hint: 'Waiting for the first collection.' },
  disabled: { label: 'Disabled', tone: 'unknown', hint: 'Collection is disabled in the configuration.' },
  unreachable: { label: 'Down', tone: 'critical', hint: 'The host did not respond over SSH.' },
  auth_failed: { label: 'Auth failed', tone: 'critical', hint: 'The SSH server rejected the configured keys.' },
  host_key_unknown: {
    label: 'Key not trusted',
    tone: 'warning',
    hint: 'The SSH host key must be approved by an administrator.',
  },
  host_key_changed: {
    label: 'Key changed',
    tone: 'critical',
    hint: 'The SSH host key differs from the trusted key. Collection stopped.',
  },
};

export function statusInfo(s: string): StatusInfo {
  return MAP[s as HostStatus] ?? { label: s, tone: 'unknown', hint: '' };
}

/** Sort order: most urgent first. */
export function statusRank(s: string): number {
  const order: HostStatus[] = [
    'host_key_changed',
    'unreachable',
    'auth_failed',
    'critical',
    'host_key_unknown',
    'warning',
    'pending',
    'ok',
    'disabled',
  ];
  const i = order.indexOf(s as HostStatus);
  return i === -1 ? order.length : i;
}

export function severityTone(sev: string): Tone {
  if (sev === 'critical') return 'critical';
  if (sev === 'warning') return 'warning';
  return 'unknown';
}

/** Tone for a utilisation percentage (disk, memory). */
export function usageTone(p: number | null | undefined, warn = 80, crit = 90): Tone {
  if (p === null || p === undefined) return 'unknown';
  if (p >= crit) return 'critical';
  if (p >= warn) return 'warning';
  return 'ok';
}
