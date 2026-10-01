import { api, ApiError, onAuthProblem, setCsrf } from './api';
import type { Meta, Role, User } from './types';

export const session = $state({
  meta: null as Meta | null,
  user: null as User | null,
  loaded: false,
  /** A step the user must complete before using the app. */
  blocked: null as null | 'password' | 'totp',
  error: null as string | null,
});

const rank: Record<Role, number> = { viewer: 0, operator: 1, admin: 2 };

export function can(role: Role): boolean {
  return !!session.user && rank[session.user.role] >= rank[role];
}

export async function loadSession(): Promise<void> {
  try {
    session.meta = await api.get<Meta>('/meta');
  } catch (e) {
    session.error = e instanceof ApiError ? e.message : 'Cannot reach the server.';
    session.loaded = true;
    return;
  }
  try {
    const me = await api.get<{ user: User; csrf: string }>('/auth/me');
    setUser(me.user, me.csrf);
  } catch {
    session.user = null;
  }
  session.loaded = true;
}

export function setUser(user: User, csrf: string): void {
  setCsrf(csrf);
  session.user = user;
  session.blocked = user.must_change_password ? 'password' : null;
  session.error = null;
}

export function clearUser(): void {
  setCsrf('');
  session.user = null;
  session.blocked = null;
}

onAuthProblem((e) => {
  if (e.status === 401) clearUser();
  else if (e.code === 'password_change_required') session.blocked = 'password';
  else if (e.code === 'totp_enrollment_required') session.blocked = 'totp';
});

export async function logout(): Promise<void> {
  try {
    await api.post('/auth/logout');
  } finally {
    clearUser();
  }
}
