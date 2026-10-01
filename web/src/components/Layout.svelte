<script lang="ts">
  import type { Snippet } from 'svelte';
  import {
    LayoutGrid,
    Bell,
    ShieldCheck,
    History,
    GitCompareArrows,
    Play,
    Users,
    KeyRound,
    Monitor,
    ScrollText,
    Moon,
    Sun,
    Keyboard,
    LogOut,
    UserRound,
    Menu,
    X,
  } from '@lucide/svelte';
  import { router } from '../lib/router.svelte';
  import { session, can, logout } from '../lib/session.svelte';
  import { fleet } from '../lib/fleet.svelte';
  import { theme, toggleTheme } from '../lib/theme.svelte';
  import { PRODUCT_NAME } from '../lib/brand';

  interface Props {
    children: Snippet;
    onhelp: () => void;
  }
  let { children, onhelp }: Props = $props();
  let navOpen = $state(false);
  let menuOpen = $state(false);

  const hosts = $derived(Object.values(fleet.hosts));
  const firing = $derived(hosts.reduce((n, h) => n + h.alerts.critical + h.alerts.warning, 0));
  const critical = $derived(hosts.reduce((n, h) => n + h.alerts.critical, 0));
  const keyIssues = $derived(hosts.filter((h) => h.status === 'host_key_unknown' || h.status === 'host_key_changed').length);

  function active(prefix: string): boolean {
    if (prefix === '/') return router.path === '/' || router.path.startsWith('/hosts/');
    return router.path === prefix || router.path.startsWith(`${prefix}/`);
  }

  $effect(() => {
    // Close the mobile navigation after navigating.
    void router.path;
    navOpen = false;
    menuOpen = false;
  });
</script>

<div class="shell" class:nav-open={navOpen}>
  <a class="skip" href="#main">Skip to content</a>
  <aside class="sidebar" aria-label="Main navigation">
    <div class="brand">
      <span class="logo" aria-hidden="true"></span>
      <span class="name">{PRODUCT_NAME}</span>
      <button class="btn ghost icon small close" type="button" aria-label="Close navigation" onclick={() => (navOpen = false)}>
        <X size={16} />
      </button>
    </div>
    <nav>
      <a href="/" aria-current={active('/') ? 'page' : undefined}><LayoutGrid size={16} />Fleet</a>
      <a href="/alerts" aria-current={active('/alerts') ? 'page' : undefined}>
        <Bell size={16} />Alerts
        {#if firing > 0}<span class="count" class:critical={critical > 0}>{firing}</span>{/if}
      </a>
      <a href="/security" aria-current={active('/security') ? 'page' : undefined}><ShieldCheck size={16} />Security</a>
      <a href="/changes" aria-current={active('/changes') ? 'page' : undefined}><History size={16} />Changes</a>
      <a href="/compare" aria-current={active('/compare') ? 'page' : undefined}><GitCompareArrows size={16} />Compare</a>
      {#if session.meta?.features.actions && can('operator')}
        <a href="/actions" aria-current={active('/actions') ? 'page' : undefined}><Play size={16} />Actions</a>
      {/if}
      {#if can('admin')}
        <p class="section">Administration</p>
        <a href="/admin/users" aria-current={active('/admin/users') ? 'page' : undefined}><Users size={16} />Users</a>
        <a href="/admin/hostkeys" aria-current={active('/admin/hostkeys') ? 'page' : undefined}>
          <KeyRound size={16} />Host keys
          {#if keyIssues > 0}<span class="count critical">{keyIssues}</span>{/if}
        </a>
        <a href="/admin/display" aria-current={active('/admin/display') ? 'page' : undefined}><Monitor size={16} />Wall display</a>
        <a href="/admin/audit" aria-current={active('/admin/audit') ? 'page' : undefined}><ScrollText size={16} />Audit log</a>
      {/if}
    </nav>
    <div class="foot muted">
      {#if session.meta}v{session.meta.version}{#if session.meta.features.local_mode} · local mode{/if}{/if}
    </div>
  </aside>
  <button class="scrim" type="button" aria-label="Close navigation" tabindex="-1" onclick={() => (navOpen = false)}></button>

  <div class="main-col">
    <header class="topbar">
      <button class="btn ghost icon menu" type="button" aria-label="Open navigation" onclick={() => (navOpen = true)}>
        <Menu size={18} />
      </button>
      <span class="spacer"></span>
      <span class="live" class:off={!fleet.live} title={fleet.live ? 'Receiving live updates' : 'Live updates disconnected; reconnecting'}>
        <span class="dot" aria-hidden="true"></span>{fleet.live ? 'Live' : 'Reconnecting'}
      </span>
      <button class="btn ghost icon" type="button" onclick={onhelp} title="Keyboard shortcuts (?)" aria-label="Keyboard shortcuts">
        <Keyboard size={16} />
      </button>
      <button class="btn ghost icon" type="button" onclick={toggleTheme} title="Toggle theme" aria-label={theme.dark ? 'Switch to light theme' : 'Switch to dark theme'}>
        {#if theme.dark}<Sun size={16} />{:else}<Moon size={16} />{/if}
      </button>
      <div class="user">
        <button class="btn ghost" type="button" aria-haspopup="menu" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>
          <UserRound size={16} />
          <span class="uname">{session.user?.username}</span>
          <span class="role muted">{session.user?.role}</span>
        </button>
        {#if menuOpen}
          <div class="menu-pop panel" role="menu">
            <a role="menuitem" href="/settings"><UserRound size={14} />Account</a>
            <button role="menuitem" type="button" onclick={() => void logout()}><LogOut size={14} />Sign out</button>
          </div>
        {/if}
      </div>
    </header>
    <main id="main" tabindex="-1">
      {@render children()}
    </main>
  </div>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: var(--sidebar) 1fr;
    min-height: 100vh;
  }
  .skip {
    position: absolute;
    left: -9999px;
  }
  .skip:focus {
    left: 8px;
    top: 8px;
    z-index: 200;
    padding: 6px 10px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
  }
  .sidebar {
    position: sticky;
    top: 0;
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-right: 1px solid var(--border);
    z-index: 30;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    height: var(--topbar);
    padding: 0 14px;
    border-bottom: 1px solid var(--border);
  }
  .logo {
    width: 18px;
    height: 18px;
    border: 2px solid var(--accent);
    border-radius: 50%;
    position: relative;
  }
  .logo::after {
    content: '';
    position: absolute;
    inset: 6px;
    background: var(--accent);
    border-radius: 50%;
  }
  .name {
    font-weight: 600;
    letter-spacing: 0.01em;
    flex: 1;
  }
  .close {
    display: none;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 8px;
    overflow-y: auto;
    flex: 1;
  }
  nav a {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 32px;
    padding: 0 10px;
    border-radius: var(--radius);
    color: var(--text-2);
    font-weight: 500;
  }
  nav a:hover {
    background: var(--surface-2);
    color: var(--text);
    text-decoration: none;
  }
  nav a[aria-current='page'] {
    background: var(--surface-3);
    color: var(--text);
  }
  nav a :global(svg) {
    flex: none;
  }
  .section {
    margin: 14px 10px 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .count {
    margin-left: auto;
    min-width: 20px;
    padding: 0 6px;
    border-radius: 10px;
    background: var(--warning-wash);
    color: var(--text);
    font-size: 11px;
    font-weight: 600;
    line-height: 18px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .count.critical {
    background: var(--critical-wash);
    color: var(--critical-text);
  }
  .foot {
    padding: 10px 14px;
    font-size: 11px;
    border-top: 1px solid var(--border);
  }
  .scrim {
    display: none;
  }
  .main-col {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .topbar {
    position: sticky;
    top: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    gap: 4px;
    height: var(--topbar);
    padding: 0 12px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }
  .topbar .spacer {
    flex: 1;
  }
  .menu {
    display: none;
  }
  .live {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-right: 8px;
    font-size: 12px;
    color: var(--text-2);
  }
  .live .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ok);
  }
  .live.off .dot {
    background: var(--warning);
  }
  .user {
    position: relative;
  }
  .uname {
    font-weight: 500;
  }
  .role {
    font-size: 11.5px;
  }
  .menu-pop {
    position: absolute;
    right: 0;
    top: 34px;
    min-width: 160px;
    padding: 4px;
    display: flex;
    flex-direction: column;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.14);
  }
  .menu-pop a,
  .menu-pop button {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .menu-pop a:hover,
  .menu-pop button:hover {
    background: var(--surface-2);
    text-decoration: none;
  }
  main {
    outline: none;
    min-width: 0;
  }

  @media (max-width: 900px) {
    .shell {
      grid-template-columns: 1fr;
    }
    .sidebar {
      position: fixed;
      left: 0;
      top: 0;
      bottom: 0;
      width: 260px;
      transform: translateX(-100%);
      transition: transform var(--t-fast);
    }
    .nav-open .sidebar {
      transform: none;
    }
    .nav-open .scrim {
      display: block;
      position: fixed;
      inset: 0;
      z-index: 25;
      border: 0;
      background: var(--overlay);
    }
    .close,
    .menu {
      display: inline-flex;
    }
    .role {
      display: none;
    }
  }
</style>
