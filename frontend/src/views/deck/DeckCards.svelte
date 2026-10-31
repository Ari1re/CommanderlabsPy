<script lang="ts">
  import { api } from '../../lib/api';
  import { app } from '../../lib/store.svelte';
  import type { CardView, Deck, Entry, Zone } from '../../lib/types';
  import { entryImage, KIND_ORDER, money, price } from '../../lib/util';
  import Icon from '../../components/Icon.svelte';
  import Mana from '../../components/Mana.svelte';

  let { deck, onchange }: { deck: Deck; onchange: (d: Deck) => void } = $props();

  let view = $state<'list' | 'visual'>('list');
  let sort = $state<'cmc' | 'name'>('cmc');
  let hovered = $state<CardView | null>(null);
  let menuFor = $state<string | null>(null);

  // --- add-card search ----------------------------------------------------
  let query = $state('');
  let results = $state<CardView[]>([]);
  let active = $state(0);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function search() {
    clearTimeout(timer);
    const q = query.trim();
    if (q.length < 2) {
      results = [];
      return;
    }
    timer = setTimeout(async () => {
      try {
        results = await api<CardView[]>('search_cards', { query: q, limit: 12 });
        active = 0;
      } catch {
        results = [];
      }
    }, 200);
  }

  async function edit(args: Record<string, unknown>, message?: string) {
    try {
      const next = await api<Deck>('edit_deck', { name: deck.name, ...args });
      onchange(next);
      if (message) app.toast(message, 'success');
    } catch (err) {
      app.error(err);
    }
  }

  async function add(card: CardView, zone: Zone = 'main') {
    await edit({ action: 'add', card: card.name, zone }, `Added ${card.name}.`);
    query = '';
    results = [];
  }

  function onkey(e: KeyboardEvent) {
    if (!results.length) return;
    if (e.key === 'ArrowDown') {
      active = (active + 1) % results.length;
      e.preventDefault();
    } else if (e.key === 'ArrowUp') {
      active = (active - 1 + results.length) % results.length;
      e.preventDefault();
    } else if (e.key === 'Enter') {
      add(results[active]);
      e.preventDefault();
    } else if (e.key === 'Escape') {
      results = [];
    }
  }

  // --- grouping -------------------------------------------------------------
  interface Group {
    title: string;
    entries: Entry[];
    count: number;
  }

  function sorted(list: Entry[]) {
    return [...list].sort((a, b) =>
      sort === 'cmc'
        ? (a.card?.cmc ?? 99) - (b.card?.cmc ?? 99) || a.name.localeCompare(b.name)
        : a.name.localeCompare(b.name),
    );
  }

  let groups = $derived.by<Group[]>(() => {
    const byKind = new Map<string, Entry[]>();
    for (const e of deck.entries) {
      const key = e.zone === 'commander' ? 'Commander' : e.zone === 'sideboard' ? 'Sideboard' : (e.card?.kind ?? 'Other');
      if (!byKind.has(key)) byKind.set(key, []);
      byKind.get(key)!.push(e);
    }
    const order = [...KIND_ORDER, 'Sideboard'];
    return order
      .filter((k) => byKind.has(k))
      .map((k) => {
        const entries = sorted(byKind.get(k)!);
        return { title: k === 'Other' ? 'Unknown cards' : k, entries, count: entries.reduce((n, e) => n + e.quantity, 0) };
      });
  });

  let previewCard = $derived(hovered ?? deck.entries.find((e) => e.zone === 'commander')?.card ?? null);

  function key(e: Entry) {
    return `${e.zone}:${e.name}`;
  }
</script>

<svelte:window onclick={() => (menuFor = null)} />

<div class="toolbar">
  <div class="add">
    <Icon name="search" size={17} />
    <input
      class="input"
      placeholder="Add a card"
      bind:value={query}
      oninput={search}
      onkeydown={onkey}
      aria-label="Search cards to add"
      autocomplete="off"
    />
    {#if results.length}
      <ul class="results" role="listbox">
        {#each results as c, i (c.oracle_id)}
          <li>
            <button class:active={i === active} onmousedown={() => add(c)} onmouseenter={() => (hovered = c)}>
              <span class="r-name">{c.name}</span>
              <Mana cost={c.mana_cost} size={14} />
              <span class="small muted r-type">{c.kind}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  <div class="segmented" role="group" aria-label="View">
    <button aria-pressed={view === 'list'} onclick={() => (view = 'list')}>List</button>
    <button aria-pressed={view === 'visual'} onclick={() => (view = 'visual')}>Images</button>
  </div>
  <div class="segmented" role="group" aria-label="Sort">
    <button aria-pressed={sort === 'cmc'} onclick={() => (sort = 'cmc')}>Mana value</button>
    <button aria-pressed={sort === 'name'} onclick={() => (sort = 'name')}>Name</button>
  </div>
</div>

<div class="layout" class:visual={view === 'visual'}>
  <div class="groups">
    {#each groups as g (g.title)}
      <section class="group">
        <h3>{g.title} <span class="muted num">{g.count}</span></h3>
        {#if view === 'list'}
          <ul class="rows">
            {#each g.entries as e (key(e))}
              <li class="entry" onmouseenter={() => (hovered = e.card ? { ...e.card, image_uri: entryImage(e) } : null)}>
                <span class="qty num">{e.quantity}</span>
                <button class="name" onclick={() => e.card && app.open(e.card, { deck: deck.name, entry: e })} class:unknown={!e.card}>
                  {e.name}
                  {#if e.card?.game_changer}<span class="gc" title="Game Changer">◆</span>{/if}
                  {#if e.set_code}<span class="set small muted">{e.set_code}</span>{/if}
                  {#if e.foil}<span class="foil-tag small">foil</span>{/if}
                </button>
                {#if e.card}<Mana cost={e.card.mana_cost} size={14} />{/if}
                <span class="price small num muted">{e.card ? money(price(e.card, app.currency), app.currency) : 'not found'}</span>
                <span class="tools">
                  {#if e.zone !== 'commander'}
                    <button class="btn ghost small icon" aria-label="Add one" onclick={() => edit({ action: 'add', card: e.name, zone: e.zone })}><Icon name="plus" size={15} /></button>
                    <button class="btn ghost small icon" aria-label="Remove one" onclick={() => edit({ action: 'remove', card: e.name, zone: e.zone, qty: 1 })}><Icon name="minus" size={15} /></button>
                  {/if}
                  <span class="menu-wrap">
                    <button class="btn ghost small icon" aria-label="More for {e.name}" onclick={(ev) => { ev.stopPropagation(); menuFor = menuFor === key(e) ? null : key(e); }}><Icon name="more" size={15} /></button>
                    {#if menuFor === key(e)}
                      <div class="menu" role="menu">
                        {#if e.zone === 'main'}
                          <button role="menuitem" onclick={() => edit({ action: 'move', card: e.name, zone: 'main', to: 'sideboard' }, `Moved ${e.name} to the sideboard.`)}>Move to sideboard</button>
                          {#if e.card?.commander_eligible}
                            <button role="menuitem" onclick={() => edit({ action: 'move', card: e.name, zone: 'main', to: 'commander', qty: 1 }, `${e.name} is now a commander.`)}>Make commander</button>
                          {/if}
                        {:else if e.zone === 'sideboard'}
                          <button role="menuitem" onclick={() => edit({ action: 'move', card: e.name, zone: 'sideboard', to: 'main' }, `Moved ${e.name} to the deck.`)}>Move to deck</button>
                        {:else}
                          <button role="menuitem" onclick={() => edit({ action: 'move', card: e.name, zone: 'commander', to: 'main' })}>Move to the 99</button>
                        {/if}
                        <button role="menuitem" class="danger" onclick={() => edit({ action: 'remove', card: e.name, zone: e.zone }, `Removed ${e.name}.`)}>Remove</button>
                      </div>
                    {/if}
                  </span>
                </span>
              </li>
            {/each}
          </ul>
        {:else}
          <div class="images">
            {#each g.entries as e (key(e))}
              <button class="image" class:foil={e.foil} onclick={() => e.card && app.open(e.card, { deck: deck.name, entry: e })} aria-label={e.name}>
                {#if entryImage(e)}
                  <img class="card-img" src={entryImage(e)} alt={e.name} loading="lazy" />
                {:else}
                  <span class="card-img placeholder">{e.name}</span>
                {/if}
                {#if e.quantity > 1}<span class="stack-qty num">×{e.quantity}</span>{/if}
              </button>
            {/each}
          </div>
        {/if}
      </section>
    {/each}
  </div>

  {#if view === 'list'}
    <aside class="preview">
      {#if previewCard?.image_uri}
        <img class="card-img" src={previewCard.image_uri} alt={previewCard.name} />
      {/if}
    </aside>
  {/if}
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
    margin-bottom: 1.25rem;
  }
  .add {
    position: relative;
    flex: 1 1 280px;
    max-width: 440px;
    display: flex;
    align-items: center;
    color: var(--muted);
  }
  .add > :global(svg) {
    position: absolute;
    left: 0.65rem;
  }
  .add input {
    padding-left: 2.1rem;
  }
  .results {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 25;
    margin: 0;
    padding: 0.3rem;
    list-style: none;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow);
  }
  .results button {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.4rem 0.55rem;
    border: 0;
    background: none;
    border-radius: var(--r-sm);
    cursor: pointer;
    text-align: left;
  }
  .results button.active,
  .results button:hover {
    background: var(--panel-2);
  }
  .r-name {
    flex: 1;
  }

  .layout {
    display: grid;
    grid-template-columns: 1fr 260px;
    gap: 2rem;
    align-items: start;
  }
  .layout.visual {
    grid-template-columns: 1fr;
  }
  .groups {
    columns: 2 320px;
    column-gap: 2rem;
  }
  .visual .groups {
    columns: auto;
  }
  .group {
    break-inside: avoid;
    margin-bottom: 1.6rem;
  }
  .group h3 {
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
    padding-bottom: 0.35rem;
    margin-bottom: 0.25rem;
    border-bottom: 1px solid var(--line-soft);
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 34px;
    padding: 0 0.3rem;
    border-radius: var(--r-sm);
  }
  .entry:hover {
    background: var(--panel);
  }
  .qty {
    width: 1.4rem;
    text-align: right;
    color: var(--muted);
    font-size: 0.9rem;
  }
  .name {
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name:hover {
    color: var(--brass-strong);
  }
  .name.unknown {
    color: var(--danger);
  }
  .set {
    margin-left: 0.35rem;
    font-size: 0.72rem;
  }
  .foil-tag {
    margin-left: 0.3rem;
    font-size: 0.7rem;
    color: var(--brass-strong);
  }
  .image.foil :global(img) {
    box-shadow: 0 0 0 2px var(--brass);
  }
  .gc {
    color: var(--brass);
    font-size: 0.7rem;
    margin-left: 0.25rem;
  }
  .price {
    width: 4.6rem;
    text-align: right;
  }
  .tools {
    display: flex;
    opacity: 0;
  }
  .entry:hover .tools,
  .entry:focus-within .tools {
    opacity: 1;
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 30;
    display: grid;
    min-width: 170px;
    padding: 0.3rem;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow);
  }
  .menu button {
    padding: 0.45rem 0.6rem;
    border: 0;
    background: none;
    text-align: left;
    border-radius: var(--r-sm);
    cursor: pointer;
  }
  .menu button:hover {
    background: var(--panel-2);
  }
  .menu .danger {
    color: var(--danger);
  }

  .preview {
    position: sticky;
    top: 4.5rem;
  }

  .images {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 0.75rem;
  }
  .image {
    position: relative;
    padding: 0;
    border: 0;
    background: none;
    cursor: zoom-in;
  }
  .placeholder {
    display: grid;
    place-items: center;
    padding: 0.5rem;
    text-align: center;
    color: var(--muted);
  }
  .stack-qty {
    position: absolute;
    right: 6px;
    top: 28%;
    padding: 0.05rem 0.45rem;
    border-radius: 99px;
    background: var(--ink);
    border: 1px solid var(--brass);
    color: var(--brass-strong);
    font-weight: 700;
    font-size: 0.85rem;
  }

  @media (max-width: 1100px) {
    .layout {
      grid-template-columns: 1fr;
    }
    .preview {
      display: none;
    }
  }
  @media (hover: none) {
    .entry {
      min-height: 46px;
    }
    .name {
      align-self: stretch;
    }
    .tools :global(.btn.small.icon) {
      width: 38px;
      min-height: 38px;
    }
    .tools {
      opacity: 1;
    }
    .price {
      display: none;
    }
  }
  @media (max-width: 640px) {
    .images {
      grid-template-columns: repeat(3, 1fr);
      gap: 0.4rem;
    }
  }
</style>
