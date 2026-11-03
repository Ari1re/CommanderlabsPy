<!--
A card-name box with suggestions from the card database. Picking a
suggestion (click, or Enter on the highlighted one) calls `onpick`; Enter
with no suggestions picks what was typed, so the backend can still look it up.
Used by the Build screen's requests (cards to include, combos, leave out).
-->
<script lang="ts">
  import { api } from '../lib/api';
  import type { CardView } from '../lib/types';
  import Mana from './Mana.svelte';

  let {
    placeholder = 'Card name',
    label,
    onpick,
  }: { placeholder?: string; label: string; onpick: (name: string) => void } = $props();

  let query = $state('');
  let results = $state<CardView[]>([]);
  let active = $state(0);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;

  function search() {
    clearTimeout(timer);
    const q = query.trim();
    if (q.length < 2) {
      results = [];
      return;
    }
    const mine = ++seq;
    timer = setTimeout(async () => {
      try {
        const found = await api<CardView[]>('search_cards', { query: q, limit: 8 });
        if (mine === seq) {
          results = found;
          active = 0;
        }
      } catch {
        results = [];
      }
    }, 200);
  }

  function pick(name: string) {
    if (!name.trim()) return;
    onpick(name.trim());
    query = '';
    results = [];
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' && results.length) {
      active = (active + 1) % results.length;
      e.preventDefault();
    } else if (e.key === 'ArrowUp' && results.length) {
      active = (active - 1 + results.length) % results.length;
      e.preventDefault();
    } else if (e.key === 'Enter') {
      pick(results[active]?.name ?? query);
      e.preventDefault();
    } else if (e.key === 'Escape') {
      results = [];
    }
  }
</script>

<div class="card-input">
  <input
    class="input"
    {placeholder}
    aria-label={label}
    bind:value={query}
    oninput={search}
    onkeydown={onkey}
    onblur={() => setTimeout(() => (results = []), 150)}
    autocomplete="off"
  />
  {#if results.length}
    <ul class="results" role="listbox" aria-label="{label} suggestions">
      {#each results as c, i (c.oracle_id)}
        <li role="option" aria-selected={i === active}>
          <button type="button" class:active={i === active} onmousedown={(e) => { e.preventDefault(); pick(c.name); }}>
            <span class="r-name">{c.name}</span>
            <Mana cost={c.mana_cost} size={13} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .card-input {
    position: relative;
    flex: 1 1 200px;
    min-width: 0;
  }
  .results {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 30;
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
    color: inherit;
    font: inherit;
  }
  .results button.active,
  .results button:hover {
    background: var(--panel-2);
  }
  .r-name {
    flex: 1;
    min-width: 0;
  }
</style>
