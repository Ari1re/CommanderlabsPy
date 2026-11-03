<!--
Commander search box for the Build screen and "change commander"
(`commander_suggestions`, with Pauper Commander's own rules).
-->
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
  /** Why the list is empty, shown under the box instead of silence. */
  let message = $state('');
  let seq = 0;

  function search() {
    clearTimeout(timer);
    const q = query.trim();
    if (q.length < 2) {
      results = [];
      message = '';
      return;
    }
    const mine = ++seq;
    timer = setTimeout(async () => {
      try {
        const found = await api<CardView[]>('commander_suggestions', { query: q, format });
        if (mine !== seq) return; // a newer search is on its way
        results = found;
        active = 0;
        open = true;
        message = found.length
          ? ''
          : format === 'pdh'
            ? `No eligible commander matches "${q}". PDH commanders are creatures printed at uncommon.`
            : `No commander matches "${q}".`;
      } catch (err) {
        if (mine !== seq) return;
        results = [];
        message = (err as Error).message;
      }
    }, 250);
  }

  function pick(card: CardView) {
    onpick(card);
    query = '';
    results = [];
    message = '';
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
          <!-- mousedown keeps focus in the box; click picks (works for taps too). -->
          <button onmousedown={(e) => e.preventDefault()} onclick={() => pick(card)} class:active={i === active}>
            <span class="thumb" style:background-image={card.image_uri ? `url("${artCrop(card.image_uri)}")` : 'none'}></span>
            <span class="name">{card.name}</span>
            <Identity identity={card.color_identity} size={10} />
          </button>
        </li>
      {/each}
    </ul>
  {:else if message}
    <p class="small msg">{message}</p>
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
  .msg {
    margin: 0.4rem 0 0;
    color: var(--muted);
  }
</style>
