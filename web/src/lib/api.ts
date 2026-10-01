// Minimal client for the JSON API. Adds the CSRF header to state-changing
// requests and turns error bodies into ApiError.

let csrfToken = '';

export function setCsrf(token: string): void {
  csrfToken = token;
}

export class ApiError extends Error {
  status: number;
  code: string;
  retryAfter: number | null;

  constructor(status: number, code: string, message: string, retryAfter: number | null = null) {
    super(message);
    this.status = status;
    this.code = code;
    this.retryAfter = retryAfter;
  }
}

type Listener = (e: ApiError) => void;
const unauthorizedListeners: Listener[] = [];

/** Called when any request fails with 401 or a forced account step. */
export function onAuthProblem(l: Listener): void {
  unauthorizedListeners.push(l);
}

async function request<T>(method: string, path: string, body?: unknown, signal?: AbortSignal): Promise<T> {
  const headers: Record<string, string> = { Accept: 'application/json' };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (method !== 'GET' && csrfToken) headers['X-CSRF-Token'] = csrfToken;
  let resp: Response;
  try {
    resp = await fetch(`/api/v1${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: 'same-origin',
      signal,
    });
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') throw e;
    throw new ApiError(0, 'network', 'The server could not be reached. Check that it is running and try again.');
  }
  if (resp.status === 204) return undefined as T;
  const text = await resp.text();
  let data: unknown = null;
  if (text) {
    try {
      data = JSON.parse(text);
    } catch {
      data = null;
    }
  }
  if (!resp.ok) {
    const d = (data ?? {}) as { error?: string; message?: string; retry_after?: number };
    const err = new ApiError(
      resp.status,
      d.error ?? 'http_error',
      d.message ?? `Request failed with HTTP ${resp.status}.`,
      d.retry_after ?? null,
    );
    if (
      resp.status === 401 ||
      err.code === 'password_change_required' ||
      err.code === 'totp_enrollment_required'
    ) {
      for (const l of unauthorizedListeners) l(err);
    }
    throw err;
  }
  return data as T;
}

export const api = {
  get: <T>(path: string, signal?: AbortSignal) => request<T>('GET', path, undefined, signal),
  post: <T>(path: string, body: unknown = {}) => request<T>('POST', path, body),
  patch: <T>(path: string, body: unknown) => request<T>('PATCH', path, body),
  del: <T>(path: string) => request<T>('DELETE', path),
};

/** Encodes a path segment. */
export function seg(s: string): string {
  return encodeURIComponent(s);
}

/** Builds a query string, skipping empty values. */
export function qs(params: Record<string, string | number | null | undefined>): string {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v !== null && v !== undefined && v !== '') p.set(k, String(v));
  }
  const s = p.toString();
  return s ? `?${s}` : '';
}
