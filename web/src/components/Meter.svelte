<script lang="ts">
  import { usageTone } from '../lib/status';
  import { pct } from '../lib/format';

  interface Props {
    value: number | null;
    warn?: number;
    crit?: number;
    label?: string;
    width?: number;
    showValue?: boolean;
  }
  let { value, warn = 80, crit = 90, label = '', width = 64, showValue = true }: Props = $props();
  const tone = $derived(usageTone(value, warn, crit));
  const fill = $derived(value === null ? 0 : Math.max(0, Math.min(100, value)));
</script>

<span class="meter-wrap" title={label ? `${label}: ${pct(value, 2)}` : pct(value, 2)}>
  <span
    class={`meter tone-${tone}`}
    style:width={`${width}px`}
    role="meter"
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={value ?? undefined}
    aria-label={label || 'Usage'}
  >
    <span class="fill" style:width={`${fill}%`}></span>
  </span>
  {#if showValue}<span class="num val">{pct(value)}</span>{/if}
</span>

<style>
  .meter-wrap {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .meter {
    display: inline-block;
    height: 6px;
    border-radius: 3px;
    background: var(--accent-wash);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    border-radius: 3px;
    background: var(--accent);
  }
  .tone-warning {
    background: var(--warning-wash);
  }
  .tone-warning .fill {
    background: var(--warning);
  }
  .tone-critical {
    background: var(--critical-wash);
  }
  .tone-critical .fill {
    background: var(--critical);
  }
  .tone-unknown .fill {
    background: var(--unknown);
  }
  .val {
    min-width: 42px;
    text-align: right;
    font-size: 12px;
  }
</style>
