<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { app, go } from '../lib/store.svelte';
  import type { DeckSummary } from '../lib/types';
  import { artCrop, money, plural, timeAgo } from '../lib/util';
  import Icon from '../components/Icon.svelte';
  import Identity from '../components/Identity.svelte';
  import ImportDialog from '../components/ImportDialog.svelte';

  let decks = $state<DeckSummary[] | null>(null);
  let filter = $state('');
  let importing = $state(false);

  async function load() {
    try {
      decks = await api<DeckSummary[]>('list_decks');
    } catch (err) {
      app.error(err);
      decks = [];
    }
  }

  onMount(load);

  let shown = $derived(
    (decks ?? []).filter((d) => {
      const q = filter.trim().toLowerCase();
      return !q || d.name.toLowerCase().includes(q) || d.commanders.some((c) => c.toLowerCase().includes(q));
    }),
  );

  function open(d: DeckSummary) {
    go({ page: 'deck', name: d.name, tab: 'cards' });
  }
</script>

<header class="head">
  <h1>Your decks</h1>
  <div class="row">
    {#if decks && decks.length > 4}
      <label class="search">
        <Icon name="search" size={17} />
        <input class="input" placeholder="Find a deck or commander" bind:value={filter} />
      </label>
    {/if}
    <button class="btn" onclick={() => (importing = true)}><Icon name="plus" size={18} /> Add a deck</button>
    <a class="btn primary" href="#/build"><Icon name="build" size={18} /> Build a deck</a>
  </div>
</header>

{#if decks === null}
  <div class="grid">
    {#each Array(6) as _, i (i)}<div class="tile skeleton"></div>{/each}
  </div>
{:else if decks.length === 0}
  <div class="empty">
    <h2>No decks yet</h2>
    <p class="dim">
      Paste a list or a Moxfield or Archidekt link, or tell Commander Lab a commander and it will
      build a full 100-card deck at the bracket you choose.
    </p>
    <div class="row">
      <button class="btn" onclick={() => (importing = true)}>Add a deck</button>
      <a class="btn primary" href="#/build">Build a deck</a>
    </div>
  </div>
{:else}
  <div class="grid">
    {#each shown as d (d.name)}
      <button class="tile" onclick={() => open(d)}>
        <div class="art" style:background-image={d.image ? `url("${artCrop(d.image)}")` : 'none'}></div>
        <div class="body">
          <h3>{d.name}</h3>
          <p class="cmd dim">{d.commanders.join(' + ') || 'No commander set'}</p>
          <div class="meta small">
            <Identity identity={d.identity} />
            <span class:warn={d.cards !== 100}>{plural(d.cards, 'card')}</span>
            {#if d.price_usd > 0}<span class="num">{money(d.price_usd, 'usd')}</span>{/if}
            <span class="spacer"></span>
            <span class="muted">{timeAgo(d.modified_ms)}</span>
          </div>
        </div>
      </button>
    {/each}
  </div>
  {#if shown.length === 0}
    <p class="dim">No decks match “{filter}”.</p>
  {/if}
{/if}

{#if importing}
  <ImportDialog
    onclose={() => (importing = false)}
    ondone={(name) => {
      importing = false;
      go({ page: 'deck', name, tab: 'cards' });
    }}
  />
{/if}

<style>
  .head {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 1.75rem;
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    color: var(--muted);
  }
  .search :global(svg) {
    position: absolute;
    left: 0.65rem;
  }
  .search input {
    padding-left: 2.1rem;
    width: 15rem;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 1.1rem;
  }
  .tile {
    position: relative;
    display: grid;
    grid-template-rows: auto 1fr;
    padding: 0;
    text-align: left;
    background: var(--panel);
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    overflow: hidden;
    cursor: pointer;
    color: inherit;
  }
  .tile:hover {
    border-color: var(--brass);
  }
  .tile:hover .art {
    transform: scale(1.03);
  }
  .art {
    aspect-ratio: 16 / 9;
    background: var(--panel-2) center 22% / cover no-repeat;
    transition: transform 0.4s ease;
    mask-image: linear-gradient(to bottom, #000 62%, transparent);
  }
  .body {
    display: grid;
    gap: 0.2rem;
    padding: 0 1rem 0.9rem;
    margin-top: -1.6rem;
    position: relative;
  }
  .body h3 {
    font-size: 1.25rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cmd {
    font-size: 0.88rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    margin-top: 0.45rem;
    color: var(--text-2);
  }
  .warn {
    color: var(--danger);
  }
  .skeleton {
    height: 230px;
    background: var(--panel);
    animation: pulse 1.4s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.55;
    }
  }
  @media (max-width: 760px) {
    .search input {
      width: 100%;
    }
    .search {
      flex: 1 1 100%;
    }
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
