<!--
The card detail sheet: full image, text, prices, legality and printings;
inside a deck it can change the card's printing and foil.
-->
<script lang="ts">
  import { api } from '../lib/api';
  import type { CardView, Deck, Printing } from '../lib/types';
  import { app } from '../lib/store.svelte';
  import { entryImage, money, price } from '../lib/util';
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

  // Printing picker, only for cards that are in the open deck.
  let ctx = $derived(app.inspectEntry);
  let printings = $state<Printing[] | null>(null);
  let loadingPrintings = $state(false);
  let picking = $state(false);
  let saving = $state(false);

  let current = $derived(
    ctx?.entry.set_code ? { set: ctx.entry.set_code, num: ctx.entry.collector_number ?? '' } : null,
  );
  let image = $derived(ctx ? entryImage(ctx.entry) || card.image_uri : card.image_uri);

  async function loadPrintings() {
    picking = true;
    if (printings) return;
    loadingPrintings = true;
    try {
      printings = await api<Printing[]>('printings', { card: card.name });
    } catch (err) {
      app.error(err);
      picking = false;
    } finally {
      loadingPrintings = false;
    }
  }

  async function choose(p: Printing | null | undefined, foil?: boolean) {
    if (!ctx) return;
    saving = true;
    try {
      const args: Record<string, unknown> = { name: ctx.deck, zone: ctx.entry.zone, card: ctx.entry.name };
      if (p !== undefined) {
        args.set_code = p?.set_code ?? '';
        args.collector_number = p?.collector_number ?? '';
      }
      if (foil !== undefined) args.foil = foil;
      const deck = await api<Deck>('set_printing', args);
      const updated = deck.entries.find((e) => e.zone === ctx!.entry.zone && e.name === ctx!.entry.name);
      if (updated) app.inspectEntry = { deck: ctx.deck, entry: updated };
      app.deckUpdated?.(deck);
      if (p !== undefined) picking = false;
    } catch (err) {
      app.error(err);
    } finally {
      saving = false;
    }
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === 'Escape') onclose();
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="scrim" onclick={onclose} role="presentation"></div>
<div class="sheet" class:wide={picking} role="dialog" aria-modal="true" aria-label={card.name}>
  <button class="btn ghost icon close" onclick={onclose} aria-label="Close"><Icon name="close" /></button>

  {#if picking}
    <div class="picker">
      <div class="row">
        <button class="btn ghost small" onclick={() => (picking = false)}><Icon name="back" size={15} /> Back</button>
        <h2>Choose a printing of {card.name}</h2>
      </div>
      {#if loadingPrintings}
        <p class="muted">Loading printings from Scryfall…</p>
      {:else if printings}
        <div class="prints">
          <button class="print" class:on={!current} onclick={() => choose(null)} disabled={saving}>
            <img class="card-img" src={card.image_uri} alt="" loading="lazy" />
            <span class="small"><b>Default</b></span>
            <span class="small muted">No specific printing</span>
          </button>
          {#each printings as p (p.set_code + p.collector_number)}
            <button
              class="print"
              class:on={current?.set === p.set_code && current?.num === p.collector_number}
              onclick={() => choose(p)}
              disabled={saving}
            >
              <img class="card-img" src={p.image} alt="" loading="lazy" />
              <span class="small"><b>{p.set_name}</b></span>
              <span class="small muted">{p.set_code} #{p.collector_number} · {p.released_at.slice(0, 4)}</span>
              <span class="small num">{p.price_usd ? `$${p.price_usd}` : ''}{p.price_usd_foil ? ` · foil $${p.price_usd_foil}` : ''}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {:else}
    <img class="card-img art" class:foil={ctx?.entry.foil} src={image} alt={card.name} />
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
        <!-- In a deck, the entry's card is priced for its printing and foil. -->
        <dd class="num">{money(price(ctx?.entry.card ?? card, app.currency), app.currency)}</dd>
        <dt>EDHREC rank</dt>
        <dd class="num">{card.edhrec_rank ? `#${card.edhrec_rank.toLocaleString()}` : '—'}</dd>
        <dt>Printing</dt>
        <dd>
          {#if current}{current.set} #{current.num}{ctx?.entry.foil ? ' · foil' : ''}{:else if card.set_code}Default ({card.set_code} #{card.collector_number}){:else}Default{/if}
        </dd>
      </dl>
      {#if ctx}
        <div class="row">
          <button class="btn" onclick={loadPrintings}><Icon name="cards" size={16} /> Change printing</button>
          <label class="toggle">
            <input type="checkbox" checked={ctx.entry.foil ?? false} disabled={saving} onchange={(e) => choose(undefined, (e.target as HTMLInputElement).checked)} />
            Foil
          </label>
        </div>
      {/if}
      <div class="row">
        {#if card.tcgplayer_uri}<a class="btn small" href={card.tcgplayer_uri} target="_blank" rel="noreferrer">TCGplayer</a>{/if}
        {#if card.cardmarket_uri}<a class="btn small" href={card.cardmarket_uri} target="_blank" rel="noreferrer">Cardmarket</a>{/if}
        <a class="btn small ghost" href={`https://scryfall.com/search?q=!"${encodeURIComponent(card.name)}"`} target="_blank" rel="noreferrer">Scryfall <Icon name="external" size={14} /></a>
      </div>
    </div>
  {/if}
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
    max-width: 100vw;
    overflow-x: hidden;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: min(820px, calc(100vw - 2rem));
    max-height: calc(100vh - 2rem);
    overflow: auto;
    display: grid;
    grid-template-columns: minmax(200px, 300px) minmax(0, 1fr);
    gap: 1.5rem;
    padding: 1.5rem;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    box-shadow: var(--shadow);
  }
  .sheet.wide {
    width: min(1080px, calc(100vw - 2rem));
    grid-template-columns: minmax(0, 1fr);
  }
  .close {
    position: absolute;
    top: 0.5rem;
    right: 0.5rem;
    z-index: 2;
  }
  .info {
    display: grid;
    gap: 0.8rem;
    align-content: start;
    min-width: 0;
    overflow-wrap: anywhere;
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
    overflow-wrap: anywhere;
  }
  .art {
    max-width: 100%;
    height: auto;
  }
  .picker,
  .prints,
  .print {
    min-width: 0;
  }
  .foil {
    box-shadow: 0 0 0 2px var(--brass), 0 0 22px color-mix(in srgb, var(--brass) 45%, transparent);
  }
  dl {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 0.25rem 1rem;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
  }
  .toggle {
    display: inline-flex;
    gap: 0.4rem;
    align-items: center;
    cursor: pointer;
  }
  .toggle input {
    accent-color: var(--brass);
    width: 18px;
    height: 18px;
  }
  .picker {
    display: grid;
    gap: 1rem;
  }
  .picker .row {
    padding-right: 2.5rem;
  }
  .prints {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 0.9rem;
  }
  .print {
    display: grid;
    gap: 0.2rem;
    align-content: start;
    padding: 0.4rem;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    background: none;
    cursor: pointer;
    text-align: left;
  }
  .print:hover {
    background: var(--panel-2);
  }
  .print.on {
    border-color: var(--brass);
    background: color-mix(in srgb, var(--brass) 10%, var(--panel));
  }
  @media (max-width: 640px) {
    .sheet,
    .sheet.wide {
      top: auto;
      bottom: 0;
      left: 0;
      transform: none;
      width: 100%;
      max-height: 90vh;
      grid-template-columns: minmax(0, 1fr);
      border-radius: var(--r-md) var(--r-md) 0 0;
      padding-bottom: calc(1.5rem + var(--safe-bottom));
    }
    .art {
      width: 62%;
      margin: 0 auto;
    }
    .prints {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
