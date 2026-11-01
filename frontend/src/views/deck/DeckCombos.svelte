<script lang="ts">
  import { api, runJob } from '../../lib/api';
  import { app } from '../../lib/store.svelte';
  import type { Bracket, CardView, ComboLine } from '../../lib/types';
  import Icon from '../../components/Icon.svelte';

  let { name }: { name: string } = $props();

  interface Report {
    included: number;
    almost_included: number;
    variants: ComboLine[];
    /** Combos the deck could add, legal in its format and colors. */
    possible?: ComboLine[];
  }

  interface ComboCard {
    name: string;
    image: string;
    in_deck: boolean;
    type_line: string;
  }

  let report = $state<Report | null>(null);
  let cards = $state<Record<string, ComboCard>>({});
  let bracket = $state<Bracket | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let tab = $state<'have' | 'add'>('have');
  let show = $state<'all' | 'complete' | 'almost'>('all');
  let open = $state<string | null>(null);

  async function run() {
    busy = true;
    error = null;
    try {
      const res = await runJob<{ report: Report; bracket: Bracket; cards?: Record<string, ComboCard> }>('combos', { name });
      report = res.report;
      bracket = res.bracket;
      cards = res.cards ?? {};
    } catch (err) {
      error = (err as Error).message;
    } finally {
      busy = false;
    }
  }

  async function inspect(card: string) {
    try {
      app.open(await api<CardView>('card', { name: card }));
    } catch (err) {
      app.error(err);
    }
  }

  let shown = $derived(
    tab === 'add'
      ? (report?.possible ?? [])
      : (report?.variants ?? []).filter((v) => show === 'all' || (show === 'almost' ? v.almost : !v.almost)),
  );
</script>

{#if !report}
  <div class="empty">
    <h2>Find combos in this deck</h2>
    <p class="dim">
      Checks your list against Commander Spellbook for complete combos, combos you're one card
      away from, and combos you could add. Only cards legal in this deck's format and colors are
      suggested. The result also refines the bracket estimate.
    </p>
    {#if error}<p class="err">{error}</p>{/if}
    <button class="btn primary" onclick={run} disabled={busy}>
      <Icon name="combo" size={18} /> {busy ? 'Checking Commander Spellbook…' : 'Find combos'}
    </button>
  </div>
{:else}
  <div class="row head">
    <div class="segmented" role="group" aria-label="Combo lists">
      <button aria-pressed={tab === 'have'} onclick={() => (tab = 'have')}>
        Have or almost have ({report.included + report.almost_included})
      </button>
      <button aria-pressed={tab === 'add'} onclick={() => (tab = 'add')}>
        Could add ({report.possible?.length ?? 0})
      </button>
    </div>
    <span class="spacer"></span>
    {#if bracket}<span class="badge brass">Bracket {bracket.bracket} with combos</span>{/if}
    <button class="btn ghost icon" onclick={run} disabled={busy} aria-label="Check again"><Icon name="refresh" size={18} /></button>
  </div>
  {#if tab === 'have'}
    <div class="row head">
      <h2>{report.included} complete, {report.almost_included} one card away</h2>
      <span class="spacer"></span>
      <div class="segmented" role="group" aria-label="Filter">
        <button aria-pressed={show === 'all'} onclick={() => (show = 'all')}>All</button>
        <button aria-pressed={show === 'complete'} onclick={() => (show = 'complete')}>In the deck</button>
        <button aria-pressed={show === 'almost'} onclick={() => (show = 'almost')}>One card away</button>
      </div>
    </div>
  {:else}
    <p class="dim small head">Combos you don't run yet, legal in this deck's format and colors. Ones built around your commander come first, then the fewest cards to add.</p>
  {/if}

  {#if shown.length === 0}
    <p class="muted">No combos in this view.</p>
  {/if}

  <ul class="combos">
    {#each shown as v (v.id)}
      <li class="panel combo" class:almost={v.almost}>
        <button class="summary" onclick={() => (open = open === v.id ? null : v.id)} aria-expanded={open === v.id}>
          {#if tab === 'add'}
            <span class="badge">+{v.missing?.length ?? 0} card{(v.missing?.length ?? 0) === 1 ? '' : 's'}</span>
          {:else}
            <span class="badge" class:brass={!v.almost}>{v.almost ? '+1 card' : 'In deck'}</span>
          {/if}
          <span class="cards">{v.cards.join(' + ')}</span>
          {#if v.bracket}<span class="small muted">B{v.bracket}</span>{/if}
        </button>
        <div class="pieces" role="group" aria-label="Cards in this combo">
          {#each v.cards as c, i (c)}
            {@const info = cards[c]}
            {#if i > 0}<span class="plus" aria-hidden="true">+</span>{/if}
            <button class="piece" class:missing={info && !info.in_deck} onclick={() => inspect(c)} aria-label="{c}{info && !info.in_deck ? ' (not in your deck)' : ''}">
              {#if info?.image}
                <img class="card-img" src={info.image} alt={c} loading="lazy" />
              {:else}
                <span class="card-img placeholder">{c}</span>
              {/if}
              {#if info && !info.in_deck}<span class="need">Add this</span>{/if}
            </button>
          {/each}
        </div>
        <p class="result dim">{v.result}</p>
        {#if open === v.id}
          <div class="detail">
            <div class="row">
              {#each v.cards as c (c)}<button class="btn small" onclick={() => inspect(c)}>{c}</button>{/each}
            </div>
            {#if v.prerequisites.length}
              <h3>Before you start</h3>
              <ul>{#each v.prerequisites as p, i (i)}<li>{p}</li>{/each}</ul>
            {/if}
            {#if v.steps.length}
              <h3>Steps</h3>
              <ol>{#each v.steps as s, i (i)}<li>{s}</li>{/each}</ol>
            {/if}
            {#if v.spellbook_url}
              <a class="btn small ghost" href={v.spellbook_url} target="_blank" rel="noreferrer">Open on Commander Spellbook <Icon name="external" size={14} /></a>
            {/if}
          </div>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .head {
    margin-bottom: 1rem;
  }
  .err {
    color: var(--danger);
  }
  .combos {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.6rem;
  }
  .combo {
    padding: 0.8rem 1rem;
    display: grid;
    gap: 0.3rem;
  }
  .combo.almost {
    border-style: dashed;
  }
  .summary {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    border: 0;
    background: none;
    padding: 0;
    cursor: pointer;
    text-align: left;
  }
  .cards {
    flex: 1;
    font-weight: 700;
  }
  .result {
    font-size: 0.9rem;
    white-space: pre-line;
  }
  .pieces {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
    margin: 0.3rem 0;
  }
  .piece {
    position: relative;
    width: clamp(84px, 22vw, 132px);
    padding: 0;
    border: 0;
    background: none;
    cursor: zoom-in;
  }
  .piece.missing .card-img {
    opacity: 0.55;
    outline: 2px dashed var(--brass);
    outline-offset: 2px;
  }
  .need {
    position: absolute;
    left: 50%;
    bottom: 8%;
    transform: translateX(-50%);
    padding: 0.1rem 0.5rem;
    border-radius: 99px;
    background: var(--brass);
    color: var(--brass-ink);
    font-size: 0.72rem;
    font-weight: 700;
    white-space: nowrap;
  }
  .plus {
    font-family: var(--display);
    font-size: 1.4rem;
    color: var(--muted);
  }
  .placeholder {
    display: grid;
    place-items: center;
    padding: 0.4rem;
    font-size: 0.75rem;
    text-align: center;
    color: var(--muted);
  }
  .detail {
    display: grid;
    gap: 0.5rem;
    padding-top: 0.5rem;
    border-top: 1px solid var(--line-soft);
    margin-top: 0.3rem;
  }
  .detail ul,
  .detail ol {
    margin: 0;
    padding-left: 1.2rem;
  }
</style>
