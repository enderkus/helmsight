// Global keyboard shortcuts: `/` search, `g f` fleet, `g s` security,
// `g a` alerts, `g c` changes, `?` help.

export interface ShortcutHandlers {
  search: () => void;
  go: (where: 'fleet' | 'security' | 'alerts' | 'changes') => void;
  help: () => void;
}

export const SHORTCUTS: [string, string][] = [
  ['/', 'Focus search'],
  ['g f', 'Go to fleet'],
  ['g s', 'Go to security'],
  ['g a', 'Go to alerts'],
  ['g c', 'Go to changes'],
  ['?', 'Show keyboard shortcuts'],
  ['Esc', 'Close dialog or clear search'],
];

function typing(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable;
}

export function installShortcuts(h: ShortcutHandlers): () => void {
  let pendingG = 0;
  const onKey = (e: KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey || e.altKey || typing(e.target)) return;
    const now = Date.now();
    if (pendingG && now - pendingG < 1000) {
      pendingG = 0;
      const map: Record<string, 'fleet' | 'security' | 'alerts' | 'changes'> = {
        f: 'fleet',
        s: 'security',
        a: 'alerts',
        c: 'changes',
      };
      const where = map[e.key];
      if (where) {
        e.preventDefault();
        h.go(where);
      }
      return;
    }
    if (e.key === 'g') {
      pendingG = now;
    } else if (e.key === '/') {
      e.preventDefault();
      h.search();
    } else if (e.key === '?') {
      e.preventDefault();
      h.help();
    }
  };
  document.addEventListener('keydown', onKey);
  return () => document.removeEventListener('keydown', onKey);
}
