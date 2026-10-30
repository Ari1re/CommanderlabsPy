<script lang="ts">
  import { api, runJob, type Job } from '../lib/api';
  import { app, go } from '../lib/store.svelte';
  import type { CardView } from '../lib/types';
  import { artCrop, BRACKETS } from '../lib/util';
  import CommanderPicker from '../components/CommanderPicker.svelte';
  import Identity from '../components/Identity.svelte';
  import Icon from '../components/Icon.svelte';

  interface Theme {
    name: string;
    score: number;
    reason: string;
  }

  let commanders = $state<CardView[]>([]);
  let themes = $state<Theme[] | null>(null);
  let theme = $state('');
  let bracket = $state(3);
  let job = $state<Job | null>(null);
  let failed = $state<string | null>(null);

  let spec = $derived(commanders.map((c) => c.name).join(' + '));
  let canPair = $derived(
    commanders.length === 1 &&
      /partner|friends forever|choose a background|doctor's companion/i.test(
        commanders[0].oracle_text + ' ' + commanders[0].type_line,
      ),
  );

  async function loadThemes() {
    themes = null;
    theme = '';
    if (!commanders.length) return;
    try {
      const res = await api<{ themes: Theme[] }>('generation_themes', { commander: spec });
      themes = res.themes;
      theme = themes[0]?.name ?? '';
    } catch (err) {
      app.error(err);
      themes = [];
    }
  }

  function pick(card: CardView) {
    commanders = [...commanders.filter((c) => c.name !== card.name), card].slice(-2);
    loadThemes();
  }

  function remove(card: CardView) {
    commanders = commanders.filter((c) => c !== card);
    loadThemes();
  }

  async function build() {
    failed = null;
    job = { id: 0, kind: 'generate', subject: spec, status: 'running', percent: 0, step: 'Starting', detail: '', log: [], result: null, error: null };
    try {
      const res = await runJob<{ name: string }>('generate', { commander: spec, theme, bracket }, (j) => (job = j), 800);
      app.toast(`Built ${res.name}.`, 'success');
      go({ page: 'deck', name: res.name, tab: 'cards' });
    } catch (err) {
      failed = (err as Error).message;
    }
  }
</script>

<header class="head">
  <h1>Build a deck</h1>
  <p class="dim lede">
    Pick a commander, a power level and a play style. Commander Lab researches how the commander
    is usually built, then assembles a legal 100-card deck that matches your bracket.
  </p>
</header>

{#if job && !failed}
  <section class="building panel">
    {#if commanders[0]}
      <div class="building-art" style:background-image={`url("${artCrop(commanders[0].image_uri)}")`}></div>
    {/if}
    <div class="building-body">
      <h2>Building {spec}</h2>
      <p class="dim">{job.step}</p>
      <div class="progress"><i style:width="{job.percent}%"></i></div>
      <ol class="log small">
        {#each job.log.slice(-8) as line, i (i)}<li>{line}</li>{/each}
      </ol>
    </div>
  </section>
{:else}
  <div class="layout">
    <section class="stack">
      <h2>Commander</h2>
      {#if commanders.length < 2}
        <CommanderPicker
          label={commanders.length ? 'Partner or Background' : 'Search for a commander'}
          placeholder={commanders.length ? 'Add a partner (optional)' : 'Kefka, Court Mage'}
          onpick={pick}
        />
      {/if}
      {#if commanders.length}
        <div class="chosen">
          {#each commanders as c (c.oracle_id)}
            <figure>
              <button class="img-btn" onclick={() => (app.inspect = c)} aria-label="View {c.name}">
                <img class="card-img" src={c.image_uri} alt={c.name} />
              </button>
              <figcaption>
                <Identity identity={c.color_identity} />
                <button class="btn small ghost" onclick={() => remove(c)}>Remove</button>
              </figcaption>
            </figure>
          {/each}
        </div>
        {#if canPair}<p class="small muted">This commander can have a partner. Add one above, or leave it solo.</p>{/if}
      {/if}
    </section>

    <section class="stack">
      <h2>Power level</h2>
      <div class="brackets" role="radiogroup" aria-label="Bracket">
        {#each BRACKETS as b (b.n)}
          <button role="radio" aria-checked={bracket === b.n} class="bracket" onclick={() => (bracket = b.n)}>
            <span class="b-num">{b.n}</span>
            <span class="b-name">{b.name}</span>
            <span class="b-blurb small dim">{b.blurb}</span>
          </button>
        {/each}
      </div>

      <h2>Play style</h2>
      {#if !commanders.length}
        <p class="muted">Choose a commander to see its most popular play styles.</p>
      {:else if themes === null}
        <p class="muted">Looking up play styles…</p>
      {:else if themes.length === 0}
        <p class="muted">No themes found; the deck will follow the commander's own synergy.</p>
      {:else}
        <div class="themes">
          {#each themes as t (t.name)}
            <button class="theme" aria-pressed={theme === t.name} onclick={() => (theme = t.name)} title={t.reason}>
              {t.name}
            </button>
          {/each}
        </div>
        {#if theme}
          <p class="small dim">{themes.find((t) => t.name === theme)?.reason}</p>
        {/if}
      {/if}

      {#if failed}
        <p class="error">{failed}</p>
      {/if}

      <div>
        <button class="btn primary big" disabled={!commanders.length || !app.status?.card_data_ready} onclick={build}>
          <Icon name="build" size={18} /> Build the deck
        </button>
      </div>
      <p class="small muted">
        Every deck is checked before it's saved: exactly 100 cards, singleton, inside the
        commander's colors, Commander-legal, and locked to the bracket you picked.
      </p>
    </section>
  </div>
{/if}

<style>
  .head {
    display: grid;
    gap: 0.5rem;
    margin-bottom: 2rem;
  }
  .lede {
    max-width: 60ch;
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(260px, 340px) 1fr;
    gap: 3rem;
    align-items: start;
  }
  .chosen {
    display: flex;
    gap: 1rem;
  }
  figure {
    margin: 0;
    width: 220px;
    display: grid;
    gap: 0.5rem;
  }
  figcaption {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .img-btn {
    padding: 0;
    border: 0;
    background: none;
    cursor: zoom-in;
  }
  .brackets {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 0.5rem;
  }
  .bracket {
    display: grid;
    align-content: start;
    gap: 0.15rem;
    text-align: left;
    padding: 0.8rem;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--panel);
    cursor: pointer;
  }
  .bracket[aria-checked='true'] {
    border-color: var(--brass);
    background: color-mix(in srgb, var(--brass) 10%, var(--panel));
  }
  .b-num {
    font-family: var(--display);
    font-size: 1.9rem;
    line-height: 1;
    color: var(--brass-strong);
  }
  .b-name {
    font-weight: 700;
  }
  .themes {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }
  .theme {
    padding: 0.4rem 0.85rem;
    border-radius: 99px;
    border: 1px solid var(--line);
    background: var(--panel);
    cursor: pointer;
    font-weight: 700;
    font-size: 0.9rem;
    color: var(--text-2);
  }
  .theme[aria-pressed='true'] {
    border-color: var(--brass);
    color: var(--brass-strong);
  }
  .big {
    min-height: 46px;
    padding: 0 1.4rem;
    font-size: 1rem;
  }
  .error {
    color: var(--danger);
  }

  .building {
    display: grid;
    grid-template-columns: 320px 1fr;
    overflow: hidden;
  }
  .building-art {
    background: center / cover;
    min-height: 260px;
  }
  .building-body {
    display: grid;
    gap: 0.75rem;
    align-content: start;
    padding: 1.5rem;
  }
  .log {
    margin: 0;
    padding-left: 1.1rem;
    color: var(--muted);
    display: grid;
    gap: 0.2rem;
  }

  @media (max-width: 900px) {
    .layout {
      grid-template-columns: 1fr;
      gap: 2rem;
    }
    .brackets {
      grid-template-columns: 1fr;
    }
    .bracket {
      grid-template-columns: 2.2rem auto 1fr;
      align-items: baseline;
      gap: 0.6rem;
    }
    .building {
      grid-template-columns: 1fr;
    }
    .building-art {
      min-height: 160px;
    }
    figure {
      width: 46%;
    }
  }
</style>
