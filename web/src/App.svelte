<script lang="ts">
  import { onMount, type Component } from 'svelte';
  import { router, match } from './lib/router.svelte';
  import { session, loadSession } from './lib/session.svelte';
  import { initTheme } from './lib/theme.svelte';
  import { startClock } from './lib/clock.svelte';
  import { connectFleet, disconnectFleet, loadFleet } from './lib/fleet.svelte';
  import { installShortcuts } from './lib/keyboard';
  import { PRODUCT_NAME } from './lib/brand';
  import Layout from './components/Layout.svelte';
  import Toasts from './components/Toasts.svelte';
  import ShortcutsHelp from './components/ShortcutsHelp.svelte';
  import Login from './pages/Login.svelte';
  import Setup from './pages/Setup.svelte';
  import ForcedStep from './pages/ForcedStep.svelte';
  import Display from './pages/Display.svelte';
  import Fleet from './pages/Fleet.svelte';
  import Host from './pages/Host.svelte';
  import Compare from './pages/Compare.svelte';
  import Security from './pages/Security.svelte';
  import Alerts from './pages/Alerts.svelte';
  import Changes from './pages/Changes.svelte';
  import Actions from './pages/Actions.svelte';
  import Settings from './pages/Settings.svelte';
  import Users from './pages/admin/Users.svelte';
  import HostKeys from './pages/admin/HostKeys.svelte';
  import DisplayTokens from './pages/admin/DisplayTokens.svelte';
  import Audit from './pages/admin/Audit.svelte';
  import NotFound from './pages/NotFound.svelte';

  let helpOpen = $state(false);

  type Page = Component<{ params: Record<string, string> }>;
  const routes: [string, Page, string][] = [
    ['/', Fleet, 'Fleet'],
    ['/hosts/:name', Host, 'Host'],
    ['/compare', Compare, 'Compare'],
    ['/security', Security, 'Security'],
    ['/alerts', Alerts, 'Alerts'],
    ['/changes', Changes, 'Changes'],
    ['/actions', Actions, 'Actions'],
    ['/settings', Settings, 'Account'],
    ['/admin/users', Users, 'Users'],
    ['/admin/hostkeys', HostKeys, 'Host keys'],
    ['/admin/display', DisplayTokens, 'Display tokens'],
    ['/admin/audit', Audit, 'Audit log'],
  ];

  const current = $derived.by(() => {
    for (const [pattern, component, title] of routes) {
      const params = match(pattern, router.path);
      if (params) return { component, params, title };
    }
    return { component: NotFound as Page, params: {}, title: 'Not found' };
  });

  const signedIn = $derived(!!session.user && !session.blocked);

  $effect(() => {
    const page = router.path === '/display' ? 'Display' : signedIn ? current.title : 'Sign in';
    document.title = `${page} · ${PRODUCT_NAME}`;
  });

  $effect(() => {
    if (signedIn) {
      void loadFleet();
      connectFleet();
    } else {
      disconnectFleet();
    }
  });

  onMount(() => {
    initTheme();
    startClock();
    const stopRouter = router.start();
    void loadSession();
    const stopKeys = installShortcuts({
      search: () => {
        const el = document.querySelector<HTMLInputElement>('[data-search]');
        if (el) el.focus();
        else router.navigate('/?focus=search');
      },
      go: (where) => router.navigate(where === 'fleet' ? '/' : `/${where}`),
      help: () => (helpOpen = true),
    });
    return () => {
      stopRouter();
      stopKeys();
    };
  });
</script>

{#if router.path === '/display'}
  <Display />
{:else if !session.loaded}
  <div class="boot" aria-busy="true"><span class="skeleton"></span></div>
{:else if session.error && !session.meta}
  <main class="fatal">
    <h1>{PRODUCT_NAME}</h1>
    <p>{session.error}</p>
    <button class="btn" type="button" onclick={() => location.reload()}>Reload</button>
  </main>
{:else if !session.user && router.path !== '/login' && (session.meta?.setup_required || router.path === '/setup')}
  <Setup />
{:else if !session.user}
  <Login />
{:else if session.blocked}
  <ForcedStep step={session.blocked} />
{:else}
  {@const Page = current.component}
  <Layout onhelp={() => (helpOpen = true)}>
    <Page params={current.params} />
  </Layout>
{/if}

<ShortcutsHelp open={helpOpen} onclose={() => (helpOpen = false)} />
<Toasts />

<style>
  .boot {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
  }
  .boot .skeleton {
    width: 160px;
    height: 8px;
  }
  .fatal {
    max-width: 480px;
    margin: 20vh auto;
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    align-items: flex-start;
  }
</style>
