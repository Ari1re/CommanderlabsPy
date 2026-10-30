<script lang="ts">
  import type { CardView } from '../lib/types';
  import { app } from '../lib/store.svelte';
  import { money, price } from '../lib/util';
  import Mana from './Mana.svelte';
  import Icon from './Icon.svelte';

  let { card, onclose }: { card: CardView; onclose: () => void } = $props();

  const roleNames: Record<string, string> = {
    ramp: 'Ramp',
    draw: 'Card draw',
    tutor: 'Tutor',
    removal: 'Removal',
    counter: 'Counterspell',
    wipe: 'Board wipe',
    recursion: 'Recursion',
    protection: 'Protection',
    graveyard: 'Graveyard',
  };
  let roles = $derived(
    Object.entries(card.roles)
      .filter(([, v]) => v)
      .map(([k]) => roleNames[k]),
  );

  function onkey(e: KeyboardEvent) {
    if (e.key === 'Escape') onclose();
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="scrim" onclick={onclose} role="presentation"></div>
<div class="sheet" role="dialog" aria-modal="true" aria-label={card.name}>
  <button class="btn ghost icon close" onclick={onclose} aria-label="Close"><Icon name="close" /></button>
  <img class="card-img art" src={card.image_uri} alt={card.name} />
  <div class="info">
    <div class="title">
      <h2>{card.name}</h2>
      <Mana cost={card.mana_cost} size={18} />
    </div>
    <p class="dim">{card.type_line}</p>
    {#if card.oracle_text}
      <p class="oracle">{card.oracle_text}</p>
    {/if}
    <div class="row">
      {#if card.game_changer}<span class="badge brass">Game Changer</span>{/if}
      {#if !card.commander_legal}<span class="badge danger">Banned in Commander</span>{/if}
      {#each roles as r (r)}<span class="badge">{r}</span>{/each}
    </div>
    <dl>
      <dt>Price</dt>
      <dd class="num">{money(price(card, app.currency), app.currency)}</dd>
      <dt>EDHREC rank</dt>
      <dd class="num">{card.edhrec_rank ? `#${card.edhrec_rank.toLocaleString()}` : '—'}</dd>
      {#if card.set_code}
        <dt>Printing</dt>
        <dd>{card.set_code} {card.collector_number}</dd>
      {/if}
    </dl>
    <div class="row">
      {#if card.tcgplayer_uri}<a class="btn small" href={card.tcgplayer_uri} target="_blank" rel="noreferrer">TCGplayer</a>{/if}
      {#if card.cardmarket_uri}<a class="btn small" href={card.cardmarket_uri} target="_blank" rel="noreferrer">Cardmarket</a>{/if}
      <a class="btn small ghost" href={`https://scryfall.com/search?q=!"${encodeURIComponent(card.name)}"`} target="_blank" rel="noreferrer">Scryfall <Icon name="external" size={14} /></a>
    </div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(10, 6, 16, 0.6);
    z-index: 80;
  }
  .sheet {
    position: fixed;
    z-index: 81;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: min(820px, calc(100vw - 2rem));
    max-height: calc(100vh - 2rem);
    overflow: auto;
    display: grid;
    grid-template-columns: minmax(200px, 300px) 1fr;
    gap: 1.5rem;
    padding: 1.5rem;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    box-shadow: var(--shadow);
  }
  .close {
    position: absolute;
    top: 0.5rem;
    right: 0.5rem;
  }
  .info {
    display: grid;
    gap: 0.8rem;
    align-content: start;
  }
  .title {
    display: flex;
    gap: 0.75rem;
    align-items: baseline;
    flex-wrap: wrap;
    padding-right: 2.5rem;
  }
  .oracle {
    white-space: pre-line;
    max-width: 60ch;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.25rem 1rem;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
  }
  @media (max-width: 640px) {
    .sheet {
      top: auto;
      bottom: 0;
      left: 0;
      transform: none;
      width: 100%;
      max-height: 88vh;
      grid-template-columns: 1fr;
      border-radius: var(--r-md) var(--r-md) 0 0;
      padding-bottom: calc(1.5rem + var(--safe-bottom));
    }
    .art {
      width: 62%;
      margin: 0 auto;
    }
  }
</style>
