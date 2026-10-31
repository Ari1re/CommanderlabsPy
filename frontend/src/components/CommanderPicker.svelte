<script lang="ts">
  import { api } from '../lib/api';
  import type { CardView } from '../lib/types';
  import { artCrop } from '../lib/util';
  import Identity from './Identity.svelte';

  let {
    label = 'Commander',
    placeholder = 'Search for a commander',
    format = 'commander',
    onpick,
  }: { label?: string; placeholder?: string; format?: string; onpick: (card: CardView) => void } = $props();

  let query = $state('');
  let results = $state<CardView[]>([]);
  let open = $state(false);
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
        results = await api<CardView[]>('commander_suggestions', { query: q, format });
        active = 0;
        open = true;
      } catch {
        results = [];
      }
    }, 250);
  }

  function pick(card: CardView) {
    onpick(card);
    query = '';
    results = [];
    open = false;
  }

  function onkey(e: KeyboardEvent) {
    if (!open || !results.length) return;
    if (e.key === 'ArrowDown') {
      active = (active + 1) % results.length;
      e.preventDefault();
    } else if (e.key === 'ArrowUp') {
      active = (active - 1 + results.length) % results.length;
      e.preventDefault();
    } else if (e.key === 'Enter') {
      pick(results[active]);
      e.preventDefault();
    } else if (e.key === 'Escape') {
      open = false;
    }
  }
</script>

<div class="picker">
  <label class="field">
    <span>{label}</span>
    <input
      class="input"
      bind:value={query}
      oninput={search}
      onkeydown={onkey}
      onfocus={() => (open = results.length > 0)}
      {placeholder}
      autocomplete="off"
      role="combobox"
      aria-expanded={open}
      aria-controls="cmd-results"
    />
  </label>
  {#if open && results.length}
    <ul id="cmd-results" role="listbox">
      {#each results as card, i (card.oracle_id)}
        <li role="option" aria-selected={i === active}>
          <button onmousedown={() => pick(card)} class:active={i === active}>
            <span class="thumb" style:background-image={card.image_uri ? `url("${artCrop(card.image_uri)}")` : 'none'}></span>
            <span class="name">{card.name}</span>
            <Identity identity={card.color_identity} size={10} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .picker {
    position: relative;
  }
  ul {
    position: absolute;
    z-index: 20;
    left: 0;
    right: 0;
    top: calc(100% + 4px);
    margin: 0;
    padding: 0.3rem;
    list-style: none;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow);
    max-height: 340px;
    overflow: auto;
  }
  button {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    width: 100%;
    padding: 0.35rem 0.5rem;
    border: 0;
    background: transparent;
    border-radius: var(--r-sm);
    cursor: pointer;
    text-align: left;
  }
  button.active,
  button:hover {
    background: var(--panel-2);
  }
  .thumb {
    width: 52px;
    height: 34px;
    border-radius: 4px;
    background: var(--panel-2) center / cover;
    flex-shrink: 0;
  }
  .name {
    flex: 1;
  }
</style>
