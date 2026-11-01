<script lang="ts">
  import { api, openExternal } from '../../lib/api';
  import { app } from '../../lib/store.svelte';
  import type { CardView, Deck, Zone } from '../../lib/types';
  import { money } from '../../lib/util';
  import Icon from '../../components/Icon.svelte';
  import Mana from '../../components/Mana.svelte';

  let { deck, onchange }: { deck: Deck; onchange: (d: Deck) => void } = $props();

  interface Result {
    name: string;
    image: string;
    mana_cost: string;
    type_line: string;
    set: string;
    set_name: string;
    collector_number: string;
    rarity: string;
    price_usd: string | null;
    card: CardView | null;
  }

  let query = $state('');
  let order = $state('edhrec');
  let dir = $state<'auto' | 'asc' | 'desc'>('auto');
  let unique = $state<'cards' | 'art' | 'prints'>('cards');
  let fits = $state(true);
  let results = $state<Result[]>([]);
  let total = $state(0);
  let page = $state(1);
  let hasMore = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let warnings = $state<string[]>([]);
  let searched = $state('');
  /** Searches that find cards, when this one found none. */
  let suggestions = $state<{ q: string; reason: string; total: number }[]>([]);
  let adding = $state<string | null>(null);

  const ORDERS = [
    ['edhrec', 'Popularity'],
    ['name', 'Name'],
    ['cmc', 'Mana value'],
    ['usd', 'Price'],
    ['released', 'Release date'],
    ['rarity', 'Rarity'],
    ['color', 'Color'],
    ['power', 'Power'],
  ] as const;

  /** Quick filters, like Moxfield's search shortcuts. */
  const QUICK = [
    ['Creatures', 't:creature'],
    ['Instants', 't:instant'],
    ['Sorceries', 't:sorcery'],
    ['Artifacts', 't:artifact'],
    ['Enchantments', 't:enchantment'],
    ['Lands', 't:land -t:basic'],
    ['Ramp', 'otag:ramp'],
    ['Card draw', 'otag:card-advantage'],
    ['Removal', 'otag:removal'],
    ['Board wipes', 'otag:board-wipe'],
    ['Tutors', 'otag:tutor'],
    ['Counterspells', 'otag:counterspell'],
    ['Mana value ≤ 2', 'mv<=2'],
    ['Under $1', 'usd<1'],
  ] as const;

  let commanders = $derived(deck.entries.filter((e) => e.zone === 'commander'));
  let identity = $derived.by(() => {
    const used = new Set(
      (deck.format === 'canlander' ? deck.entries : commanders).flatMap((e) => (e.card?.color_identity ?? '').split('')),
    );
    return ['W', 'U', 'B', 'R', 'G'].filter((c) => used.has(c)).join('');
  });
  /** What "fits this deck" adds to the search: the format's legality and,
   * for commander formats, the commander's colors. */
  let fitFilter = $derived.by(() => {
    const f = deck.format === 'pdh' ? 'f:paupercommander' : deck.format === 'canlander' ? 'f:vintage' : 'f:commander';
    if (deck.format === 'canlander' || !commanders.length) return f;
    return `${f} id<=${identity || 'C'}`;
  });

  let counts = $derived.by(() => {
    const out = new Map<string, Partial<Record<Zone, number>>>();
    for (const e of deck.entries) {
      const key = e.name.toLowerCase();
      const z = out.get(key) ?? {};
      z[e.zone] = (z[e.zone] ?? 0) + e.quantity;
      out.set(key, z);
    }
    return out;
  });

  function fullQuery() {
    return fits ? `${query.trim()} ${fitFilter}`.trim() : query.trim();
  }

  async function search(more = false) {
    const q = fullQuery();
    if (!query.trim()) return;
    busy = true;
    error = null;
    const nextPage = more ? page + 1 : 1;
    try {
      const res = await api<{
        total: number;
        has_more: boolean;
        cards: Result[];
        warnings: string[];
        query?: string;
        suggestions?: { q: string; reason: string; total: number }[];
      }>('scryfall_search', {
        q: query.trim(),
        filter: fits ? fitFilter : '',
        order,
        dir,
        unique,
        page: nextPage,
      });
      results = more ? [...results, ...res.cards] : res.cards;
      total = res.total;
      hasMore = res.has_more;
      warnings = res.warnings ?? [];
      if (!more) suggestions = res.suggestions ?? [];
      page = nextPage;
      searched = res.query ?? q;
    } catch (err) {
      error = (err as Error).message;
      if (!more) results = [];
    } finally {
      busy = false;
    }
  }

  function quick(filter: string) {
    query = query.includes(filter) ? query.replace(filter, '').replace(/\s+/g, ' ').trim() : `${query} ${filter}`.trim();
    search();
  }

  async function add(r: Result, zone: Zone) {
    const key = `${r.name}:${zone}`;
    adding = key;
    try {
      let next = await api<Deck>('edit_deck', { name: deck.name, action: 'add', card: r.name, zone });
      // Every-printing searches keep the printing you picked.
      if (unique === 'prints' && r.set && r.collector_number) {
        next = await api<Deck>('set_printing', { name: deck.name, zone, card: r.name, set_code: r.set, collector_number: r.collector_number });
      }
      onchange(next);
      const where = zone === 'main' ? 'the deck' : zone === 'sideboard' ? 'the sideboard' : 'Considering';
      app.toast(`Added ${r.name} to ${where}.`, 'success');
    } catch (err) {
      app.error(err);
    } finally {
      adding = null;
    }
  }

  async function inspect(r: Result) {
    try {
      app.open(r.card ?? (await api<CardView>('card', { name: r.name })));
    } catch (err) {
      app.error(err);
    }
  }
</script>

<form class="bar" onsubmit={(e) => { e.preventDefault(); search(); }}>
  <span class="icon"><Icon name="search" size={18} /></span>
  <input
    class="input q"
    bind:value={query}
    placeholder={'Scryfall search: t:creature o:"draw a card" mv<=3'}
    aria-label="Search cards with Scryfall syntax"
    autocomplete="off"
    autocapitalize="off"
    spellcheck="false"
  />
  <button class="btn primary" disabled={busy || !query.trim()}>{busy ? 'Searching…' : 'Search'}</button>
</form>

<div class="controls">
  <label class="check" title={fitFilter}>
    <input type="checkbox" bind:checked={fits} onchange={() => query.trim() && search()} />
    Only cards that fit this deck <span class="muted small">({fitFilter})</span>
  </label>
  <label class="small sel">Sort
    <select class="input" bind:value={order} onchange={() => query.trim() && search()}>
      {#each ORDERS as [id, label] (id)}<option value={id}>{label}</option>{/each}
    </select>
  </label>
  <button class="btn small ghost" onclick={() => { dir = dir === 'desc' ? 'asc' : 'desc'; if (query.trim()) search(); }} aria-label="Sort direction">
    {dir === 'desc' ? '↓ Descending' : dir === 'asc' ? '↑ Ascending' : '↕ Default order'}
  </button>
  <label class="small sel">Show
    <select class="input" bind:value={unique} onchange={() => query.trim() && search()}>
      <option value="cards">One per card</option>
      <option value="art">Every artwork</option>
      <option value="prints">Every printing</option>
    </select>
  </label>
  <button class="btn small ghost" onclick={() => openExternal('https://scryfall.com/docs/syntax')}>Syntax guide <Icon name="external" size={13} /></button>
</div>

<div class="quick" role="group" aria-label="Quick filters">
  {#each QUICK as [label, filter] (label)}
    <button class="chip" aria-pressed={query.includes(filter)} onclick={() => quick(filter)}>{label}</button>
  {/each}
</div>

{#if error}
  <p class="err">{error}</p>
{:else if searched}
  <p class="small dim">
    {total.toLocaleString()} card{total === 1 ? '' : 's'} for <code>{searched}</code>{#if warnings.length} · {warnings.join(' ')}{/if}
  </p>
  {#if total === 0}
    <div class="nothing panel">
      {#if suggestions.length}
        <b>Nothing matched. These find cards:</b>
        <div class="suggest">
          {#each suggestions as sg (sg.q)}
            <button class="sg" onclick={() => { query = sg.q; search(); }}>
              <code>{sg.q}</code>
              <span class="small dim">{sg.total.toLocaleString()} cards · {sg.reason}</span>
            </button>
          {/each}
        </div>
      {:else}
        <b>Nothing matched.</b>
        <p class="small dim">
          Card text uses current Oracle wording, so search for what cards actually say (<code>o:"can't be blocked"</code>,
          not <code>o:unblockable</code>), or try a Scryfall tag like <code>otag:removal</code>. Turning off
          "Only cards that fit this deck" searches everything.
        </p>
      {/if}
    </div>
  {/if}
{:else}
  <div class="help panel">
    <b>Search like on Scryfall</b>
    <ul class="small dim">
      <li><code>t:creature o:enters</code> creatures with enter abilities · <code>o:"can't be blocked"</code> unblockable</li>
      <li><code>o:"-1/-1 counter" mv&lt;=3</code> cheap cards that use -1/-1 counters</li>
      <li><code>otag:ramp c:g</code> green ramp · <code>is:commander</code> possible commanders</li>
      <li><code>art:dragon</code> · <code>set:dmu</code> · <code>r:common</code> · <code>usd&lt;2</code> · <code>year&lt;2000</code></li>
    </ul>
    <p class="small muted">"Only cards that fit this deck" adds the format and the commander's colors for you.</p>
  </div>
{/if}

<div class="grid">
  {#each results as r, i (r.name + r.set + r.collector_number + i)}
    {@const have = counts.get(r.name.toLowerCase())}
    <article class="result">
      <button class="img" onclick={() => inspect(r)} aria-label="View {r.name}">
        {#if r.image}
          <img class="card-img" src={r.image} alt={r.name} loading="lazy" />
        {:else}
          <span class="card-img placeholder">{r.name}</span>
        {/if}
        {#if have}
          <span class="have">
            {#if have.commander}Commander{/if}
            {#if have.main}Deck ×{have.main}{/if}
            {#if have.sideboard}Side ×{have.sideboard}{/if}
            {#if have.considering}Considering{/if}
          </span>
        {/if}
      </button>
      <div class="meta">
        <div class="name-row">
          <span class="name">{r.name}</span>
          <Mana cost={r.mana_cost} size={12} />
        </div>
        <span class="small muted">{unique === 'cards' ? r.type_line : `${r.set_name} · ${r.set} ${r.collector_number}`}{r.price_usd ? ` · ${money(Number(r.price_usd), 'usd')}` : ''}</span>
        <div class="adds">
          <button class="btn small primary" disabled={adding === `${r.name}:main`} onclick={() => add(r, 'main')} title="Add to the deck"><Icon name="plus" size={14} /> Deck</button>
          <button class="btn small" disabled={adding === `${r.name}:sideboard`} onclick={() => add(r, 'sideboard')} title="Add to the sideboard">Side</button>
          <button class="btn small" disabled={adding === `${r.name}:considering`} onclick={() => add(r, 'considering')} title="Add to Considering">Consider</button>
        </div>
      </div>
    </article>
  {/each}
</div>

{#if hasMore}
  <div class="more">
    <button class="btn" disabled={busy} onclick={() => search(true)}>{busy ? 'Loading…' : `Show more (${results.length} of ${total.toLocaleString()})`}</button>
  </div>
{/if}

<style>
  .nothing {
    padding: 0.8rem 1rem;
    display: grid;
    gap: 0.5rem;
    margin-bottom: 1rem;
  }
  .nothing p {
    margin: 0;
  }
  .suggest {
    display: grid;
    gap: 0.4rem;
  }
  .sg {
    display: grid;
    gap: 0.15rem;
    text-align: left;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--panel-2);
    color: var(--text);
    cursor: pointer;
    font: inherit;
  }
  .sg:hover {
    border-color: var(--brass);
  }
  .sg code {
    word-break: break-word;
  }
  .bar {
    position: relative;
    display: flex;
    gap: 0.5rem;
    margin-bottom: 0.75rem;
  }
  .bar .icon {
    position: absolute;
    left: 0.75rem;
    top: 50%;
    transform: translateY(-50%);
    color: var(--muted);
    pointer-events: none;
  }
  .q {
    flex: 1;
    min-width: 0;
    padding-left: 2.4rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.95rem;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.6rem 1rem;
    margin-bottom: 0.75rem;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    cursor: pointer;
  }
  .check input {
    accent-color: var(--brass);
    width: 18px;
    height: 18px;
  }
  .sel {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }
  .sel select {
    min-height: 36px;
    padding: 0 0.5rem;
  }
  .quick {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin-bottom: 1rem;
  }
  .chip {
    padding: 0.3rem 0.7rem;
    border-radius: 99px;
    border: 1px solid var(--line);
    background: var(--panel);
    color: var(--text-2);
    cursor: pointer;
    font-weight: 700;
    font-size: 0.82rem;
  }
  .chip[aria-pressed='true'] {
    border-color: var(--brass);
    color: var(--brass-strong);
  }
  .err {
    color: var(--danger);
  }
  code {
    font-size: 0.85em;
    padding: 0 0.25rem;
    border-radius: 4px;
    background: var(--panel-2);
    overflow-wrap: anywhere;
  }
  .help {
    padding: 0.8rem 1rem;
    margin-bottom: 1rem;
  }
  .help ul {
    margin: 0.4rem 0;
    padding-left: 1.1rem;
    display: grid;
    gap: 0.25rem;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(168px, 42vw), 1fr));
    gap: 1rem;
  }
  .result {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 0.4rem;
    align-content: start;
    min-width: 0;
  }
  .img {
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
    font-size: 0.85rem;
  }
  .have {
    position: absolute;
    left: 6px;
    top: 6px;
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
    padding: 0.05rem 0.45rem;
    border-radius: 99px;
    background: rgba(14, 10, 20, 0.85);
    border: 1px solid var(--brass);
    color: #fff;
    font-size: 0.68rem;
    font-weight: 700;
  }
  .meta {
    display: grid;
    gap: 0.2rem;
    min-width: 0;
  }
  .name-row {
    display: flex;
    gap: 0.35rem;
    align-items: baseline;
    justify-content: space-between;
  }
  .name {
    font-weight: 700;
    font-size: 0.9rem;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta > .small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .adds {
    display: flex;
    gap: 0.3rem;
    flex-wrap: wrap;
  }
  .adds .btn {
    flex: 1;
    min-width: 0;
    justify-content: center;
    padding: 0 0.4rem;
  }
  .more {
    display: flex;
    justify-content: center;
    margin-top: 1.25rem;
  }
  @media (max-width: 640px) {
    .grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 0.75rem;
    }
  }
</style>
