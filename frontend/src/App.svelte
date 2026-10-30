<script lang="ts">
  import { onMount } from 'svelte';
  import { app, go } from './lib/store.svelte';
  import { runJob } from './lib/api';
  import Icon from './components/Icon.svelte';
  import CardSheet from './components/CardSheet.svelte';
  import Library from './views/Library.svelte';
  import Build from './views/Build.svelte';
  import DeckView from './views/DeckView.svelte';
  import Settings from './views/Settings.svelte';

  let ready = $state(false);

  onMount(async () => {
    await app.load();
    ready = true;
  });

  async function updateCards() {
    app.updatingCards = { percent: 0, step: 'Starting' };
    try {
      const res = await runJob('update_cards', {}, (job) => {
        app.updatingCards = { percent: job.percent, step: job.detail || job.step };
      });
      app.toast(`Card data updated: ${res.cards.toLocaleString()} cards.`, 'success');
      await app.refreshStatus();
    } catch (err) {
      app.error(err);
    } finally {
      app.updatingCards = null;
    }
  }

  const nav = [
    { page: 'library', label: 'Decks', icon: 'library' },
    { page: 'build', label: 'Build', icon: 'build' },
    { page: 'settings', label: 'Settings', icon: 'settings' },
  ] as const;

  let section = $derived(app.route.page === 'deck' ? 'library' : app.route.page);
</script>

<div class="shell">
  <nav class="rail" aria-label="Main">
    <a class="mark" href="#/" aria-label="Commander Lab home">
      <img src="/icon.svg" alt="" width="38" height="38" />
    </a>
    {#each nav as item (item.page)}
      <a
        class="nav-item"
        href={item.page === 'library' ? '#/' : `#/${item.page}`}
        aria-current={section === item.page ? 'page' : undefined}
      >
        <Icon name={item.icon} size={22} />
        <span>{item.label}</span>
      </a>
    {/each}
  </nav>

  <main class="main">
    {#if ready && app.status && !app.status.card_data_ready}
      <section class="setup">
        <div>
          <h2>Download the card database</h2>
          <p class="dim">
            Commander Lab needs Scryfall's card data before it can analyze or build decks. It's
            about a minute, and you can refresh it any time from Settings.
          </p>
        </div>
        {#if app.updatingCards}
          <div class="setup-progress">
            <div class="progress"><i style:width="{app.updatingCards.percent}%"></i></div>
            <span class="small dim">{app.updatingCards.step}</span>
          </div>
        {:else}
          <button class="btn primary" onclick={updateCards}>
            <Icon name="download" size={18} /> Download card data
          </button>
        {/if}
      </section>
    {/if}

    {#if !ready}
      <div class="loading"></div>
    {:else if app.route.page === 'library'}
      <Library />
    {:else if app.route.page === 'build'}
      <Build />
    {:else if app.route.page === 'settings'}
      <Settings onUpdateCards={updateCards} />
    {:else if app.route.page === 'deck'}
      {#key app.route.name}
        <DeckView name={app.route.name} />
      {/key}
    {/if}
  </main>

  <nav class="tabbar" aria-label="Main">
    {#each nav as item (item.page)}
      <a
        href={item.page === 'library' ? '#/' : `#/${item.page}`}
        aria-current={section === item.page ? 'page' : undefined}
        onclick={() => go({ page: item.page } as any)}
      >
        <Icon name={item.icon} size={22} />
        <span>{item.label}</span>
      </a>
    {/each}
  </nav>
</div>

{#if app.inspect}
  <CardSheet card={app.inspect} onclose={() => (app.inspect = null)} />
{/if}

<div class="toasts" role="status" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast {t.kind}">{t.text}</div>
  {/each}
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: var(--rail) 1fr;
    min-height: 100vh;
  }

  .rail {
    position: sticky;
    top: 0;
    height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.35rem;
    padding: 1rem 0;
    border-right: 1px solid var(--line-soft);
    background: var(--ink-2);
  }
  .mark {
    margin-bottom: 1rem;
  }
  .nav-item {
    display: grid;
    justify-items: center;
    gap: 0.15rem;
    width: 62px;
    padding: 0.55rem 0;
    border-radius: var(--r-md);
    color: var(--muted);
    text-decoration: none;
    font-size: 0.74rem;
    font-weight: 700;
  }
  .nav-item:hover {
    color: var(--text);
    background: var(--panel);
  }
  .nav-item[aria-current='page'] {
    color: var(--brass-strong);
    background: var(--panel);
  }

  .main {
    min-width: 0;
    padding: clamp(1rem, 3vw, 2.25rem);
    padding-bottom: 4rem;
  }

  .setup {
    display: flex;
    align-items: center;
    gap: 1.5rem;
    flex-wrap: wrap;
    justify-content: space-between;
    padding: 1.25rem 1.5rem;
    margin-bottom: 1.75rem;
    border: 1px solid var(--brass);
    border-radius: var(--r-md);
    background: color-mix(in srgb, var(--brass) 8%, var(--panel));
  }
  .setup > div:first-child {
    max-width: 42rem;
    display: grid;
    gap: 0.35rem;
  }
  .setup-progress {
    display: grid;
    gap: 0.4rem;
    min-width: 240px;
  }

  .loading {
    height: 40vh;
  }

  .tabbar {
    display: none;
  }

  .toasts {
    position: fixed;
    right: 1rem;
    bottom: 1rem;
    display: grid;
    gap: 0.5rem;
    z-index: 100;
    max-width: min(26rem, calc(100vw - 2rem));
  }
  .toast {
    padding: 0.7rem 1rem;
    border-radius: var(--r-sm);
    background: var(--panel-2);
    border: 1px solid var(--line);
    box-shadow: var(--shadow);
    font-size: 0.92rem;
  }
  .toast.error {
    border-color: var(--danger);
  }
  .toast.success {
    border-color: var(--ok);
  }

  @media (max-width: 760px) {
    .shell {
      grid-template-columns: 1fr;
    }
    .rail {
      display: none;
    }
    .main {
      padding-bottom: calc(var(--tabbar) + var(--safe-bottom) + 2rem);
    }
    .tabbar {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      position: fixed;
      left: 0;
      right: 0;
      bottom: 0;
      height: calc(var(--tabbar) + var(--safe-bottom));
      padding-bottom: var(--safe-bottom);
      background: var(--ink-2);
      border-top: 1px solid var(--line-soft);
      z-index: 50;
    }
    .tabbar a {
      display: grid;
      place-items: center;
      align-content: center;
      gap: 0.1rem;
      color: var(--muted);
      text-decoration: none;
      font-size: 0.74rem;
      font-weight: 700;
    }
    .tabbar a[aria-current='page'] {
      color: var(--brass-strong);
    }
    .toasts {
      bottom: calc(var(--tabbar) + var(--safe-bottom) + 0.75rem);
      left: 1rem;
    }
  }
</style>
