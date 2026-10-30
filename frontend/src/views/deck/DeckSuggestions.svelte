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
      from EDHREC, your deck's gaps, and combos you're one card away from.
      {#if count >= 100}Your deck is full — add a card here, then cut one from the Cards tab.{/if}
    </p>
  </div>
  <button class="btn ghost" onclick={load} disabled={loading}><Icon name="refresh" size={16} /> Refresh</button>
</div>

<div class="filters">
  <div class="chips" role="group" aria-label="Show">
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
    <p class="muted">No suggestions found. Make sure the deck has a commander.</p>
  {/if}
  {#each sections as sec (sec.id)}
    {@const items = sec.items.filter((s) => matches(s) && !inDeck.has(s.card.name.toLowerCase()) || added.has(s.card.name))}
    {#if items.length}
      <section class="section">
        <h3>{sec.title} <span class="muted num">{items.length}</span></h3>
        <div class="grid">
          {#each items as s (s.card.oracle_id)}
            <article class="sugg" class:done={added.has(s.card.name)}>
              <button class="img" onclick={() => app.open(s.card)} aria-label="View {s.card.name}">
                <img class="card-img" src={s.card.image_uri} alt={s.card.name} loading="lazy" />
              </button>
              <div class="meta">
                <div class="name-row">
                  <span class="name">{s.card.name}</span>
                  <Mana cost={s.card.mana_cost} size={13} />
                </div>
                <p class="small muted reason">{s.reason}</p>
                <div class="row">
                  <span class="small num dim">{money(price(s.card, app.currency), app.currency)}</span>
                  <span class="spacer"></span>
                  {#if added.has(s.card.name)}
                    <span class="small added"><Icon name="check" size={15} /> Added</span>
                  {:else}
                    <button class="btn small primary" onclick={() => add(s)}><Icon name="plus" size={15} /> Add</button>
                  {/if}
                </div>
              </div>
            </article>
          {/each}
        </div>
      </section>
    {/if}
  {/each}
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
  .section {
    margin-bottom: 2rem;
  }
  .section h3 {
    margin-bottom: 0.8rem;
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(178px, 1fr));
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
    padding: 0;
    border: 0;
    background: none;
    cursor: zoom-in;
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
    min-height: 2.6em;
    line-height: 1.3;
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
</style>
