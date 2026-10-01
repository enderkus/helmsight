// A small history-API router.

class Router {
  path = $state(typeof location === 'undefined' ? '/' : location.pathname);
  search = $state(typeof location === 'undefined' ? '' : location.search);

  get query(): URLSearchParams {
    return new URLSearchParams(this.search);
  }

  navigate(to: string, opts: { replace?: boolean } = {}): void {
    const url = new URL(to, location.origin);
    if (url.origin !== location.origin) {
      location.href = to;
      return;
    }
    const target = url.pathname + url.search + url.hash;
    if (opts.replace) history.replaceState(null, '', target);
    else history.pushState(null, '', target);
    this.path = url.pathname;
    this.search = url.search;
    if (!opts.replace) window.scrollTo(0, 0);
  }

  /** Updates query parameters without adding a history entry. */
  setQuery(params: Record<string, string | null>): void {
    const q = this.query;
    for (const [k, v] of Object.entries(params)) {
      if (v === null || v === '') q.delete(k);
      else q.set(k, v);
    }
    const s = q.toString();
    this.navigate(this.path + (s ? `?${s}` : ''), { replace: true });
  }

  start(): () => void {
    const onPop = () => {
      this.path = location.pathname;
      this.search = location.search;
    };
    const onClick = (e: MouseEvent) => {
      if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
      const a = (e.target as Element | null)?.closest?.('a');
      if (!a || a.target || a.hasAttribute('download') || a.getAttribute('rel') === 'external') return;
      const href = a.getAttribute('href');
      if (!href || href.startsWith('#') || href.startsWith('mailto:')) return;
      const url = new URL(href, location.href);
      if (url.origin !== location.origin || url.pathname.startsWith('/api/')) return;
      e.preventDefault();
      this.navigate(url.pathname + url.search);
    };
    window.addEventListener('popstate', onPop);
    document.addEventListener('click', onClick);
    return () => {
      window.removeEventListener('popstate', onPop);
      document.removeEventListener('click', onClick);
    };
  }
}

export const router = new Router();

/** Matches `/hosts/:name` style patterns. Returns decoded params or null. */
export function match(pattern: string, path: string): Record<string, string> | null {
  const p = pattern.split('/').filter(Boolean);
  const s = path.split('/').filter(Boolean);
  if (p.length !== s.length) return null;
  const out: Record<string, string> = {};
  for (let i = 0; i < p.length; i++) {
    const a = p[i] as string;
    const b = s[i] as string;
    if (a.startsWith(':')) {
      try {
        out[a.slice(1)] = decodeURIComponent(b);
      } catch {
        return null;
      }
    } else if (a !== b) {
      return null;
    }
  }
  return out;
}
