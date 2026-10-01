import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import Login from '../src/pages/Login.svelte';
import { session } from '../src/lib/session.svelte';

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

describe('Login', () => {
  it('asks for a TOTP code when the server requires one, then signs in', async () => {
    session.meta = {
      product: 'helmsight',
      version: '0.1.0',
      setup_required: false,
      interval: 5,
      features: { actions: false, oidc: 'Company SSO', local_login: true, local_mode: false, prometheus: false },
    };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(json(401, { error: 'totp_required', message: 'Enter the code.' }))
      .mockResolvedValueOnce(
        json(200, {
          csrf: 'c',
          user: { id: 1, username: 'alice', role: 'admin', must_change_password: false },
        }),
      );
    vi.stubGlobal('fetch', fetchMock);
    render(Login);
    expect(screen.getByRole('link', { name: /Company SSO/ })).toHaveAttribute('href', '/api/v1/auth/oidc/start');
    await fireEvent.input(screen.getByLabelText('User name'), { target: { value: 'alice' } });
    await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'secret secret' } });
    await fireEvent.click(screen.getByRole('button', { name: /Sign in/ }));
    const code = await screen.findByLabelText(/Authentication code/);
    await fireEvent.input(code, { target: { value: '123456' } });
    await fireEvent.click(screen.getByRole('button', { name: /Sign in/ }));
    await vi.waitFor(() => expect(session.user?.username).toBe('alice'));
    const body = JSON.parse((fetchMock.mock.calls[1] as [string, RequestInit])[1].body as string);
    expect(body).toEqual({ username: 'alice', password: 'secret secret', totp: '123456' });
  });

  it('shows the server message on failure', async () => {
    session.user = null;
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(json(401, { error: 'invalid_credentials', message: 'Incorrect user name or password.' })));
    render(Login);
    await fireEvent.input(screen.getByLabelText('User name'), { target: { value: 'bob' } });
    await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'nope nope nope' } });
    await fireEvent.click(screen.getByRole('button', { name: /Sign in/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Incorrect user name or password.');
  });
});
