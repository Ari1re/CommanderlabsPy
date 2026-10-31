<script lang="ts">
  import { onMount } from 'svelte';
  import { api, runJob } from '../../lib/api';
  import { app } from '../../lib/store.svelte';
  import type { Deck, Suggestion } from '../../lib/types';
  import { money, price } from '../../lib/util';
  import Icon from '../../components/Icon.svelte';
  import Mana from '../../components/Mana.svelte';

  let { deck, onchange }: { deck: Deck; onchange: (d: Deck) => void } = $props();

  interface Section {
    id: string;
    title: string;
    items: Suggestion[];
  }

  let sections = $state<Section[] | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);
  let step = $state('');
  let role = $state<string>('all');
  let maxPrice = $state<number | null>(null);
  let added = $state<Set<string>>(new Set());
  let source = $state<string>('all');

  const SOURCE_LABEL: Record<string, string> = {
    combos: 'Combo',
    synergy: 'Synergy',
    popular: 'Popular',
    deep: 'Rules text',
    dig: 'Your search',
    gaps: 'Fills a gap',
  };

  // Deep card search for any play style or ability the user names.
  let digQuery = $state('');
  let digging = $state(false);

  async function dig() {
    const theme = digQuery.trim();
    if (!theme) return;
    digging = true;
    try {
      const res = await api<{ results: { find: { reason: string }; card: Suggestion['card'] }[] }>('deep_search', {
        deck: deck.name,
        theme,
        limit: 40,
      });
      const items = res.results.map((r) => ({ card: r.card, reason: r.find.reason }));
      sections = [{ id: 'dig', title: `Cards for "${theme}"`, items }, ...(sections ?? []).filter((x) => x.id !== 'dig')];
      source = 'dig';
      if (!items.length) app.toast(`No legal cards matched "${theme}".`);
    } catch (err) {
      app.error(err);
    } finally {
      digging = false;
    }
  }

  interface Mixed {
    card: Suggestion['card'];
    reasons: { source: string; text: string }[];
  }

  /** All suggestions in one list, interleaved across sources so the grid is
   * mixed, with duplicates merged into one card carrying every reason. */
  let mixed = $derived.by<Mixed[]>(() => {
    if (!sections) return [];
    const byName = new Map<string, Mixed>();
    const queues = sections.map((sec) => sec.items.map((it) => ({ ...it, source: sec.id })));
    const order: Mixed[] = [];
    const longest = Math.max(0, ...queues.map((q) => q.length));
    for (let i = 0; i < longest; i++) {
      for (const q of queues) {
        const it = q[i];
        if (!it) continue;
        const key = it.card.name.toLowerCase();
        const existing = byName.get(key);
        if (existing) {
          existing.reasons.push({ source: it.source, text: it.reason });
        } else {
          const m = { card: it.card, reasons: [{ source: it.source, text: it.reason }] };
          byName.set(key, m);
          order.push(m);
        }
      }
    }
    return order;
  });

  let shown = $derived(
    mixed.filter(
      (m) =>
        (added.has(m.card.name) || (!inDeck.has(m.card.name.toLowerCase()) && matches({ card: m.card, reason: '' }))) &&
        (source === 'all' || m.reasons.some((r) => r.source === source)),
    ),
  );
  let sourceCounts = $derived(
    Object.fromEntries(Object.keys(SOURCE_LABEL).map((id) => [id, mixed.filter((m) => m.reasons.some((r) => r.source === id)).length])),
  );

  const roles = [
    ['all', 'Everything'],
    ['ramp', 'Ramp'],
    ['draw', 'Card draw'],
    ['removal', 'Removal'],
    ['wipe', 'Board wipes'],
    ['counter', 'Counters'],
    ['tutor', 'Tutors'],
    ['protection', 'Protection'],
    ['recursion', 'Recursion'],
    ['creature', 'Creatures'],
    ['land', 'Lands'],
  ] as const;

  async function load() {
    loading = true;
    error = null;
    try {
      const res = await runJob<{ sections: Section[] }>('recommendations', { name: deck.name }, (j) => (step = j.step));
      sections = res.sections;
    } catch (err) {
      error = (err as Error).message;
    } finally {
      loading = false;
    }
  }

  onMount(load);

  function matches(s: Suggestion) {
    const c = s.card;
    if (maxPrice != null) {
      const p = price(c, app.currency);
      if (p != null && p > maxPrice) return false;
    }
    if (role === 'all') return true;
    if (role === 'creature') return c.kind === 'Creature';
    if (role === 'land') return c.kind === 'Land';
    return (c.roles as any)[role];
  }

  let inDeck = $derived(new Set(deck.entries.filter((e) => e.zone !== 'sideboard').map((e) => e.name.toLowerCase())));

  async function add(s: Suggestion) {
    try {
      onchange(await api<Deck>('edit_deck', { name: deck.name, action: 'add', card: s.card.name, zone: 'main' }));
      added = new Set([...added, s.card.name]);
      app.toast(`Added ${s.card.name}. ${countAfter()} cards now.`, 'success');
    } catch (err) {
      app.error(err);
    }
  }

  function countAfter() {
    return deck.entries.filter((e) => e.zone !== 'sideboard').reduce((n, e) => n + e.quantity, 0) + 1;
  }

  let count = $derived(deck.entries.filter((e) => e.zone !== 'sideboard').reduce((n, e) => n + e.quantity, 0));
</script>

<div class="head">
  <div>
    <h2>Cards you could add</h2>
    <p class="dim small">
      Picked for {deck.entries.filter((e) => e.zone === 'commander').map((e) => e.name).join(' + ') || 'this deck'}
      from EDHREC, your deck's gaps, combos you're one card away from, and a search of every card's
      rules text for your play styles{deck.format !== 'commander' ? `, legal in ${deck.format_label}` : ''}.
      {#if count >= 100}Your deck is full — add a card here, then cut one from the Cards tab.{/if}
    </p>
  </div>
  <button class="btn ghost" onclick={load} disabled={loading}><Icon name="refresh" size={16} /> Refresh</button>
</div>

<form class="dig" onsubmit={(e) => { e.preventDefault(); dig(); }}>
  <input class="input" bind:value={digQuery} placeholder="Dig deeper: any ability or play style (e.g. -1/-1 counters, wither, Elves)" aria-label="Search card rules text" />
  <button class="btn" disabled={!digQuery.trim() || digging}><Icon name="search" size={16} /> {digging ? 'Searching…' : 'Search rules text'}</button>
</form>

<div class="filters">
  <div class="chips" role="group" aria-label="Why it's suggested">
    <button class="chip" aria-pressed={source === 'all'} onclick={() => (source = 'all')}>All suggestions</button>
    {#each Object.entries(SOURCE_LABEL) as [id, label] (id)}
      {#if sourceCounts[id]}
        <button class="chip src-{id}" aria-pressed={source === id} onclick={() => (source = id)}>{label}</button>
      {/if}
    {/each}
  </div>
</div>
<div class="filters">
  <div class="chips" role="group" aria-label="Card role">
    {#each roles as [id, label] (id)}
      <button class="chip" aria-pressed={role === id} onclick={() => (role = id)}>{label}</button>
    {/each}
  </div>
  <label class="budget small">
    Max price
    <select class="input" bind:value={maxPrice}>
      <option value={null}>Any</option>
      <option value={1}>{money(1, app.currency)}</option>
      <option value={5}>{money(5, app.currency)}</option>
      <option value={10}>{money(10, app.currency)}</option>
      <option value={25}>{money(25, app.currency)}</option>
    </select>
  </label>
</div>

{#if loading && !sections}
  <p class="muted">{step || 'Finding cards for your deck…'}</p>
  <div class="grid">{#each Array(12) as _, i (i)}<div class="skeleton card-img"></div>{/each}</div>
{:else if error}
  <div class="empty">
    <p class="dim">Couldn't load suggestions: {error}</p>
    <button class="btn" onclick={load}>Try again</button>
  </div>
{:else if sections}
  {#if sections.length === 0}
    <p class="muted">No suggestions found. Try searching rules text above.</p>
  {/if}
  {#if sections.length && !shown.length}
    <p class="muted">No suggestions match these filters.</p>
  {/if}
  <div class="grid">
    {#each shown as m (m.card.oracle_id)}
      <article class="sugg" class:done={added.has(m.card.name)}>
        <button class="img" onclick={() => app.open(m.card)} aria-label="View {m.card.name}">
          {#if m.card.image_uri}
            <img class="card-img" src={m.card.image_uri} alt={m.card.name} loading="lazy" />
          {:else}
            <span class="card-img placeholder">{m.card.name}</span>
          {/if}
          <span class="tags">
            {#each m.reasons as r (r.source)}<span class="tag src-{r.source}">{SOURCE_LABEL[r.source] ?? r.source}</span>{/each}
          </span>
        </button>
        <div class="meta">
          <div class="name-row">
            <span class="name">{m.card.name}</span>
            <Mana cost={m.card.mana_cost} size={13} />
          </div>
          <p class="small muted reason" title={m.reasons.map((r) => r.text).join('\n')}>{m.reasons[0].text}</p>
          <div class="row">
            <span class="small num dim">{money(price(m.card, app.currency), app.currency)}</span>
            <span class="spacer"></span>
            {#if added.has(m.card.name)}
              <span class="small added"><Icon name="check" size={15} /> Added</span>
            {:else}
              <button class="btn small primary" onclick={() => add({ card: m.card, reason: '' })}><Icon name="plus" size={15} /> Add</button>
            {/if}
          </div>
        </div>
      </article>
    {/each}
  </div>
{/if}

<style>
  .head {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: start;
    margin-bottom: 1rem;
  }
  .head p {
    max-width: 70ch;
    margin-top: 0.25rem;
  }
  .filters {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 1.25rem;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .chip {
    padding: 0.3rem 0.75rem;
    border-radius: 99px;
    border: 1px solid var(--line);
    background: var(--panel);
    color: var(--text-2);
    cursor: pointer;
    font-weight: 700;
    font-size: 0.85rem;
  }
  .chip[aria-pressed='true'] {
    border-color: var(--brass);
    color: var(--brass-strong);
  }
  .budget {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    color: var(--text-2);
  }
  .budget select {
    width: auto;
    min-height: 34px;
    padding: 0.2rem 0.5rem;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(168px, 42vw), 1fr));
    gap: 1rem;
  }
  .sugg {
    display: grid;
    gap: 0.45rem;
    align-content: start;
  }
  .sugg.done {
    opacity: 0.55;
  }
  .img {
    position: relative;
    display: block;
    width: 100%;
    padding: 0;
    border: 0;
    background: none;
    cursor: zoom-in;
  }
  .img :global(.card-img) {
    width: 100%;
    height: auto;
    aspect-ratio: 63 / 88;
    object-fit: contain;
  }
  .placeholder {
    display: grid;
    place-items: center;
    padding: 0.5rem;
    text-align: center;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .tags {
    position: absolute;
    left: 6px;
    bottom: 6px;
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
  }
  .tag {
    padding: 0.05rem 0.45rem;
    border-radius: 99px;
    background: rgba(14, 10, 20, 0.85);
    color: #fff;
    font-size: 0.68rem;
    font-weight: 700;
    border: 1px solid rgba(255, 255, 255, 0.2);
  }
  .tag.src-combos,
  .chip.src-combos[aria-pressed='true'] {
    border-color: #d0605a;
  }
  .tag.src-synergy,
  .chip.src-synergy[aria-pressed='true'] {
    border-color: var(--brass);
  }
  .tag.src-gaps,
  .chip.src-gaps[aria-pressed='true'] {
    border-color: #4fa36a;
  }
  .sugg {
    min-width: 0;
  }
  .name-row {
    display: flex;
    gap: 0.4rem;
    align-items: baseline;
    justify-content: space-between;
  }
  .name {
    font-weight: 700;
    font-size: 0.92rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .reason {
    height: 2.6em;
    line-height: 1.3;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .added {
    display: inline-flex;
    gap: 0.25rem;
    align-items: center;
    color: var(--ok);
  }
  .skeleton {
    animation: pulse 1.4s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.55;
    }
  }
  @media (max-width: 640px) {
    .grid {
      grid-template-columns: repeat(2, 1fr);
      gap: 0.75rem;
    }
  }
  .dig {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1rem;
    flex-wrap: wrap;
  }
  .dig .input {
    flex: 1 1 16rem;
    min-width: 0;
  }
  .tag.src-deep,
  .chip.src-deep[aria-pressed='true'],
  .tag.src-dig,
  .chip.src-dig[aria-pressed='true'] {
    border-color: #4f93d6;
  }
</style>
