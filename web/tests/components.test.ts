import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import StatusBadge from '../src/components/StatusBadge.svelte';
import Meter from '../src/components/Meter.svelte';
import Sparkline from '../src/components/Sparkline.svelte';
import TimeRangePicker from '../src/components/TimeRangePicker.svelte';
import EmptyState from '../src/components/EmptyState.svelte';
import { presetRange } from '../src/lib/timerange';

describe('StatusBadge', () => {
  it('always pairs the status with a text label and an icon', () => {
    const { container } = render(StatusBadge, { status: 'host_key_changed' });
    expect(screen.getByText('Key changed')).toBeInTheDocument();
    expect(container.querySelector('svg')).not.toBeNull();
    expect(container.querySelector('.badge')).toHaveAttribute('title', expect.stringContaining('host key'));
  });
  it('keeps the label for screen readers in compact mode', () => {
    render(StatusBadge, { status: 'ok', compact: true });
    expect(screen.getByText('OK')).toHaveClass('sr-only');
  });
});

describe('Meter', () => {
  it('exposes the value and tone', () => {
    const { container } = render(Meter, { value: 93.5, label: 'Disk /' });
    const m = screen.getByRole('meter', { name: 'Disk /' });
    expect(m).toHaveAttribute('aria-valuenow', '93.5');
    expect(container.querySelector('.tone-critical')).not.toBeNull();
    expect(screen.getByText('93.5%')).toBeInTheDocument();
  });
});

describe('Sparkline', () => {
  it('draws a path for two or more points', () => {
    const { container } = render(Sparkline, { values: [10, null, 30, 50], label: 'CPU' });
    expect(screen.getByRole('img', { name: 'CPU' })).toBeInTheDocument();
    expect(container.querySelector('path.line')?.getAttribute('d')).toMatch(/^M.*L/);
  });
  it('draws a flat placeholder without data', () => {
    const { container } = render(Sparkline, { values: [], label: 'Memory' });
    expect(container.querySelector('line.none')).not.toBeNull();
  });
});

describe('TimeRangePicker', () => {
  it('reports preset choices', async () => {
    const onchange = vi.fn();
    render(TimeRangePicker, { value: presetRange('1h'), onchange });
    expect(screen.getByRole('button', { name: '1h' })).toHaveAttribute('aria-pressed', 'true');
    await fireEvent.click(screen.getByRole('button', { name: '24h' }));
    expect(onchange).toHaveBeenCalledWith(expect.objectContaining({ key: '24h', live: true }));
  });
  it('validates custom ranges', async () => {
    const onchange = vi.fn();
    render(TimeRangePicker, { value: presetRange('1h'), onchange });
    await fireEvent.click(screen.getByRole('button', { name: /Custom/ }));
    const [from, to] = screen.getAllByDisplayValue(/T/) as HTMLInputElement[];
    await fireEvent.input(from!, { target: { value: '2026-09-30T10:00' } });
    await fireEvent.input(to!, { target: { value: '2026-09-30T09:00' } });
    await fireEvent.submit(from!.closest('form')!);
    expect(screen.getByRole('alert')).toHaveTextContent('end must be after the start');
    expect(onchange).not.toHaveBeenCalled();
  });
});

describe('EmptyState', () => {
  it('renders a title', () => {
    render(EmptyState, { title: 'No hosts configured' });
    expect(screen.getByText('No hosts configured')).toBeInTheDocument();
  });
});
