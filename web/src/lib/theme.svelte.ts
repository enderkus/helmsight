// Theme preference: follows the OS unless the user picks one.

export type ThemePref = 'system' | 'light' | 'dark';

const KEY = 'theme';

function read(): ThemePref {
  try {
    const v = localStorage.getItem(KEY);
    if (v === 'light' || v === 'dark') return v;
  } catch {
    // storage unavailable
  }
  return 'system';
}

function systemDark(): boolean {
  return typeof matchMedia !== 'undefined' && matchMedia('(prefers-color-scheme: dark)').matches;
}

export const theme = $state({
  pref: 'system' as ThemePref,
  /** Resolved theme, used by charts. */
  dark: false,
  /** Increments on every change so charts can rebuild their colors. */
  version: 0,
});

function apply(): void {
  const root = document.documentElement;
  if (theme.pref === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', theme.pref);
  theme.dark = theme.pref === 'dark' || (theme.pref === 'system' && systemDark());
  theme.version++;
}

export function initTheme(): void {
  theme.pref = read();
  apply();
  if (typeof matchMedia !== 'undefined') {
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
      if (theme.pref === 'system') apply();
    });
  }
}

export function setTheme(p: ThemePref): void {
  theme.pref = p;
  try {
    if (p === 'system') localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, p);
  } catch {
    // ignore
  }
  apply();
}

export function toggleTheme(): void {
  setTheme(theme.dark ? 'light' : 'dark');
}
