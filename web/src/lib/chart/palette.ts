// Chart colors come from CSS custom properties so that light and dark
// themes use their own validated steps. Series slots are assigned in fixed
// order and follow the entity, never its rank.

export interface ChartColors {
  series: string[];
  grid: string;
  axis: string;
  label: string;
  surface: string;
  text: string;
  status: { ok: string; warning: string; critical: string; unknown: string };
}

function v(style: CSSStyleDeclaration, name: string, fallback: string): string {
  const s = style.getPropertyValue(name).trim();
  return s || fallback;
}

export function chartColors(): ChartColors {
  const s = getComputedStyle(document.documentElement);
  const series = [1, 2, 3, 4, 5, 6, 7, 8].map((i) => v(s, `--series-${i}`, '#2a78d6'));
  return {
    series,
    grid: v(s, '--chart-grid', '#e1e0d9'),
    axis: v(s, '--chart-axis', '#c3c2b7'),
    label: v(s, '--chart-label', '#6b6a65'),
    surface: v(s, '--surface', '#fcfcfb'),
    text: v(s, '--text', '#1c1c1a'),
    status: {
      ok: v(s, '--ok', '#0ca30c'),
      warning: v(s, '--warning', '#fab219'),
      critical: v(s, '--critical', '#d03b3b'),
      unknown: v(s, '--unknown', '#898781'),
    },
  };
}

/** `#rrggbb` to `rgba(r,g,b,a)`. */
export function alpha(hex: string, a: number): string {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex.trim());
  if (!m) return hex;
  const [r, g, b] = [m[1], m[2], m[3]].map((x) => parseInt(x as string, 16));
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}
