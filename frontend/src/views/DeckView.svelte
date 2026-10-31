<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { api, copyText, download, openExternal, runtime } from '../lib/api';
  import { app, go } from '../lib/store.svelte';
  import type { Analysis, Deck } from '../lib/types';
  import { artCrop, FORMATS, money, price } from '../lib/util';
  import Icon from '../components/Icon.svelte';
  import Identity from '../components/Identity.svelte';
  import Modal from '../components/Modal.svelte';
  import DeckCards from './deck/DeckCards.svelte';
  import DeckAnalysis from './deck/DeckAnalysis.svelte';
  import DeckCombos from './deck/DeckCombos.svelte';
  import DeckProxies from './deck/DeckProxies.svelte';
  import DeckSources from './deck/DeckSources.svelte';
  import DeckAgent from './deck/DeckAgent.svelte';
  import DeckSuggestions from './deck/DeckSuggestions.svelte';
  import DeckGuide from './deck/DeckGuide.svelte';
  import DeckHistory from './deck/DeckHistory.svelte';
  import ShareDialog from '../components/ShareDialog.svelte';

  let { name }: { name: string } = $props();

  let deck = $state<Deck | null>(null);
  let analysis = $state<Analysis | null>(null);
  let loadError = $state<string | null>(null);
  let menu = $state(false);
  let dialog = $state<null | 'rename' | 'duplicate' | 'delete' | 'text' | 'export' | 'share' | 'format'>(null);
  /** Bumped on every change so tabs that summarise the deck reload. */
  let version = $state(0);
  let input = $state('');
  let exportText = $state('');
  let exportPath = $state('');
  let exportTarget = $state<'moxfield' | 'archidekt' | 'tts'>('moxfield');
  let tts = $state<{ json: string; file_name: string; path: string; installed_path: string | null; cards: number } | null>(null);

  const SITES = {
    moxfield: { label: 'Moxfield', url: 'https://moxfield.com/decks/personal', steps: 'In Moxfield, choose Create deck, then Import, and paste.' },
    archidekt: { label: 'Archidekt', url: 'https://archidekt.com/', steps: 'In Archidekt, make a New deck, open Import cards and paste. The commander goes straight into the Commander category.' },
  } as const;

  async function exportTo(target: 'moxfield' | 'archidekt' | 'tts') {
    exportTarget = target;
    dialog = 'export';
    exportText = '';
    tts = null;
    try {
      if (target === 'tts') {
        tts = await api('export_tts', { name });
        return;
      }
      const res = await api<{ text: string; path: string }>(target === 'moxfield' ? 'export_moxfield' : 'export_archidekt', { name });
      exportText = res.text;
      exportPath = res.path;
      if (await copyText(res.text)) app.toast(`Copied for ${SITES[target].label}.`, 'success');
    } catch (err) {
      app.error(err);
    }
  }

  const tabs = [
    { id: 'cards', label: 'Cards', icon: 'cards' },
    { id: 'suggestions', label: 'Suggestions', icon: 'plus' },
    { id: 'analysis', label: 'Analysis', icon: 'chart' },
    { id: 'combos', label: 'Combos', icon: 'combo' },
    { id: 'play', label: 'How to play', icon: 'play' },
    { id: 'history', label: 'History', icon: 'refresh' },
    { id: 'proxies', label: 'Proxies', icon: 'print' },
    { id: 'sources', label: 'Sources', icon: 'link' },
    { id: 'agent', label: 'AI Agent', icon: 'sparkle' },
  ];
  let tab = $derived(app.route.page === 'deck' ? app.route.tab : 'cards');

  async function load() {
    try {
      deck = await api<Deck>('deck', { name });
      loadError = null;
    } catch (err) {
      loadError = (err as Error).message;
      return;
    }
    refreshAnalysis();
  }

  async function refreshAnalysis() {
    try {
      analysis = await api<Analysis>('analyze', { name });
    } catch {
      analysis = null;
    }
  }

  onMount(() => {
    load();
    app.deckUpdated = updated;
  });
  onDestroy(() => {
    if (app.deckUpdated === updated) app.deckUpdated = null;
  });

  function updated(next: Deck) {
    deck = next;
    version += 1;
    refreshAnalysis();
  }

  let commanders = $derived(deck?.entries.filter((e) => e.zone === 'commander') ?? []);
  let hero = $derived(
    commanders.find((e) => e.card?.image_uri)?.card?.image_uri ??
      deck?.entries.find((e) => e.zone === 'main' && e.card?.image_uri && !e.card.type_line.includes('Land'))?.card?.image_uri ??
      '',
  );
  let fmt = $derived(deck?.format ?? 'commander');
  let check = $derived(deck?.format_check);
  let legal = $derived(fmt === 'commander' ? (analysis?.legal ?? true) : !check?.problems.length);
  let problems = $derived(fmt === 'commander' ? (analysis?.legality_issues ?? []) : (check?.problems ?? []));

  async function setFormat(f: string) {
    try {
      const next = await api<Deck>('set_format', { name, format: f });
      dialog = null;
      updated(next);
      app.toast(`Now a ${next.format_label} deck.`, 'success');
    } catch (err) {
      app.error(err);
    }
  }
  let count = $derived(
    deck?.entries.filter((e) => e.zone !== 'sideboard').reduce((n, e) => n + e.quantity, 0) ?? 0,
  );
  let total = $derived(
    deck?.entries
      .filter((e) => e.zone !== 'sideboard')
      .reduce((sum, e) => sum + (price(e.card, app.currency) ?? 0) * e.quantity, 0) ?? 0,
  );
  let identity = $derived.by(() => {
    // No commander (Canadian Highlander): the deck's colors come from its cards.
    if (deck?.format === 'canlander' || !commanders.length) {
      const used = new Set(deck?.entries.flatMap((e) => (e.card?.color_identity ?? '').split('')) ?? []);
      return ['W', 'U', 'B', 'R', 'G'].filter((c) => used.has(c)).join('');
    }
    return analysis?.identity ?? [...new Set(commanders.flatMap((c) => (c.card?.color_identity ?? '').split('')))].join('');
  });

  async function exportMoxfield() {
    try {
      const res = await api<{ text: string; path: string }>('export_moxfield', { name });
      exportText = res.text;
      exportPath = res.path;
      const copied = await copyText(res.text);
      dialog = 'export';
      if (copied) app.toast('Moxfield list copied to the clipboard.', 'success');
    } catch (err) {
      app.error(err);
    }
  }

  async function openText() {
    menu = false;
    try {
      input = (await api<{ text: string }>('deck_text', { name })).text;
      dialog = 'text';
    } catch (err) {
      app.error(err);
    }
  }

  async function saveText() {
    try {
      await api('save_deck_text', { name, text: input, overwrite: true });
      dialog = null;
      await load();
      version += 1;
      app.toast('Decklist saved.', 'success');
    } catch (err) {
      app.error(err);
    }
  }

  async function rename(dup: boolean) {
    try {
      const res = await api<{ name: string }>(dup ? 'duplicate_deck' : 'rename_deck', { name, new_name: input });
      dialog = null;
      go({ page: 'deck', name: res.name, tab });
    } catch (err) {
      app.error(err);
    }
  }

  async function remove() {
    try {
      await api('delete_deck', { name });
      app.toast(`Deleted ${name}.`);
      go({ page: 'library' });
    } catch (err) {
      app.error(err);
    }
  }
</script>

{#if loadError}
  <div class="empty">
    <h2>Couldn't open this deck</h2>
    <p class="dim">{loadError}</p>
    <a class="btn" href="#/">Back to your decks</a>
  </div>
{:else if !deck}
  <div class="hero skeleton"></div>
{:else}
  <header class="hero">
    <div class="hero-art" style:background-image={hero ? `url("${artCrop(hero)}")` : 'none'}></div>
    <div class="hero-body">
      <a class="back small" href="#/"><Icon name="back" size={16} /> Decks</a>
      <h1>{deck.name}</h1>
      <p class="dim">
        {#if fmt === 'canlander'}Canadian Highlander · no commander{:else}{commanders.map((c) => c.name).join(' + ') || 'No commander set'}{/if}
      </p>
      <div class="facts">
        <Identity identity={identity} size={14} />
        <span class:warn={fmt === 'canlander' ? count < 100 : count !== 100}>{count}{fmt === 'canlander' ? '' : ' / 100'} cards</span>
        {#if total > 0}<span class="num">{money(total, app.currency)}</span>{/if}
        {#if fmt !== 'commander'}
          <button class="badge format" onclick={() => (dialog = 'format')} title="Change format">{deck.format_label}</button>
        {/if}
        {#if fmt === 'canlander' && check}
          <span class="badge brass" class:danger={check.points > (check.max_points ?? 10)} title={check.pointed_cards.map(([n, p]) => `${n}: ${p}`).join('\n') || 'No pointed cards'}>
            {check.points} / {check.max_points} points
          </span>
        {:else if analysis && fmt === 'commander'}
          <span class="badge brass" title={analysis.bracket.reasons.join('\n')}>
            Bracket {analysis.bracket.bracket} · {analysis.bracket.label}
          </span>
        {/if}
        {#if !legal}<span class="badge danger" title={problems.join('\n')}>Not legal</span>{/if}
      </div>
      <div class="row actions">
        <button class="btn primary" onclick={() => exportTo(exportTarget === 'tts' ? 'moxfield' : exportTarget)}><Icon name="download" size={17} /> Export</button>
        <button class="btn" onclick={() => (dialog = 'share')}><Icon name="link" size={17} /> Share</button>
        <div class="menu-wrap">
          <button class="btn icon" aria-label="More deck actions" aria-expanded={menu} onclick={() => (menu = !menu)}>
            <Icon name="more" />
          </button>
          {#if menu}
            <div class="menu" role="menu">
              <button role="menuitem" onclick={openText}><Icon name="edit" size={16} /> Edit as text</button>
              <button role="menuitem" onclick={() => { menu = false; dialog = 'format'; }}><Icon name="cards" size={16} /> Format: {deck.format_label}</button>
              <button role="menuitem" onclick={() => { menu = false; input = deck!.name; dialog = 'rename'; }}><Icon name="edit" size={16} /> Rename</button>
              <button role="menuitem" onclick={() => { menu = false; input = `${deck!.name} copy`; dialog = 'duplicate'; }}><Icon name="copy" size={16} /> Duplicate</button>
              <button role="menuitem" class="danger" onclick={() => { menu = false; dialog = 'delete'; }}><Icon name="trash" size={16} /> Delete</button>
            </div>
          {/if}
        </div>
      </div>
    </div>
  </header>

  <nav class="tabs" aria-label="Deck sections">
    {#each tabs as t (t.id)}
      <a href={`#/deck/${encodeURIComponent(name)}/${t.id}`} aria-current={tab === t.id ? 'page' : undefined}>
        <Icon name={t.icon} size={17} /> {t.label}
      </a>
    {/each}
  </nav>

  <section class="tab-body">
    {#if tab === 'cards'}
      <DeckCards {deck} onchange={updated} />
    {:else if tab === 'suggestions'}
      <DeckSuggestions {deck} onchange={updated} />
    {:else if tab === 'analysis'}
      <DeckAnalysis {name} {analysis} onchange={updated} />
    {:else if tab === 'play'}
      <DeckGuide {name} {version} />
    {:else if tab === 'history'}
      <DeckHistory {name} {version} onchange={updated} />
    {:else if tab === 'combos'}
      <DeckCombos {name} />
    {:else if tab === 'proxies'}
      <DeckProxies {name} />
    {:else if tab === 'sources'}
      <DeckSources {name} />
    {:else if tab === 'agent'}
      <DeckAgent {name} bracket={analysis?.bracket.bracket ?? 3} onsaved={(n) => go({ page: 'deck', name: n, tab: 'cards' })} />
    {/if}
  </section>
{/if}

{#if dialog === 'share'}
  <ShareDialog {name} onclose={() => (dialog = null)} />
{:else if dialog === 'format'}
  <Modal title="Deck format" onclose={() => (dialog = null)}>
    <div class="stack">
      <p class="dim">The format sets which rules the deck is checked against. Your cards don't change.</p>
      {#each FORMATS as f (f.id)}
        <button class="format-pick" class:on={fmt === f.id} onclick={() => setFormat(f.id)}>
          <b>{f.label}</b>
          <span class="small dim">{f.summary}</span>
        </button>
      {/each}
      {#if problems.length}
        <div class="problems">
          <b class="small">Problems for {deck?.format_label}</b>
          <ul class="small">{#each problems.slice(0, 12) as p (p)}<li>{p}</li>{/each}</ul>
          {#if problems.length > 12}<p class="small muted">…and {problems.length - 12} more.</p>{/if}
        </div>
      {/if}
      <p class="small muted">Canadian Highlander has no commander, so switching to it moves the commander into the deck.</p>
    </div>
  </Modal>
{:else if dialog === 'export'}
  <Modal title="Export {deck?.name ?? ''}" onclose={() => (dialog = null)} wide>
    <div class="stack">
      <div class="segmented export-tabs" role="group" aria-label="Export to">
        <button aria-pressed={exportTarget === 'moxfield'} onclick={() => exportTo('moxfield')}>Moxfield</button>
        <button aria-pressed={exportTarget === 'archidekt'} onclick={() => exportTo('archidekt')}>Archidekt</button>
        <button aria-pressed={exportTarget === 'tts'} onclick={() => exportTo('tts')}>Tabletop Simulator</button>
      </div>
      {#if exportTarget === 'tts'}
        {#if !tts}
          <p class="muted">Making the Tabletop Simulator deck…</p>
        {:else}
          {#if tts.installed_path}
            <p class="ok-line"><Icon name="check" size={16} /> Saved straight into Tabletop Simulator.</p>
            <p class="dim">In Tabletop Simulator, open <b>Objects → Saved Objects</b> and click <b>{tts.file_name.replace(/\.json$/, '')}</b> to spawn the deck ({tts.cards} cards) with its commander face up beside it.</p>
          {:else}
            <p class="dim">
              Put <b>{tts.file_name}</b> in Tabletop Simulator's <b>Saved Objects</b> folder, then spawn it from <b>Objects → Saved Objects</b>.
              The deck ({tts.cards} cards) appears with its commander face up beside it.
            </p>
            <ul class="small dim">
              <li>Windows: Documents\My Games\Tabletop Simulator\Saves\Saved Objects</li>
              <li>Linux: ~/.local/share/Tabletop Simulator/Saves/Saved Objects</li>
              <li>Mac: ~/Library/Tabletop Simulator/Saves/Saved Objects</li>
            </ul>
          {/if}
          <p class="small muted">Card images load from Scryfall, using the printings you picked. Double-faced cards flip with the number keys. Also saved to {tts.path}</p>
        {/if}
      {:else}
        <p class="dim">
          Copied to your clipboard. {SITES[exportTarget].steps} Each line keeps its set and collector number, so you get the same printings.
          Neither site lets other apps create decks in your account, so this is the quickest way in.
        </p>
        <textarea class="input" rows="10" readonly value={exportText}></textarea>
        <p class="small muted">Also saved to {exportPath}</p>
      {/if}
    </div>
    {#snippet footer()}
      {#if exportTarget === 'tts'}
        {#if tts && (!runtime.isTauri || runtime.isAndroid)}
          <button class="btn primary" onclick={() => download(tts!.file_name, tts!.json, 'application/json')}><Icon name="download" size={16} /> Download {tts.file_name}</button>
        {/if}
      {:else}
        {#if !runtime.isTauri}
          <button class="btn" onclick={() => download(`${name}.${exportTarget}.txt`, exportText)}><Icon name="download" size={16} /> Download</button>
        {/if}
        <button class="btn" onclick={async () => { await copyText(exportText); app.toast('Copied.', 'success'); }}><Icon name="copy" size={16} /> Copy again</button>
        <button class="btn primary" onclick={() => openExternal(SITES[exportTarget as 'moxfield' | 'archidekt'].url)}>Open {SITES[exportTarget as 'moxfield' | 'archidekt'].label} <Icon name="external" size={14} /></button>
      {/if}
    {/snippet}
  </Modal>
{:else if dialog === 'text'}
  <Modal title="Edit as text" onclose={() => (dialog = null)} wide>
    <textarea class="input" rows="20" bind:value={input}></textarea>
    {#snippet footer()}
      <button class="btn ghost" onclick={() => (dialog = null)}>Cancel</button>
      <button class="btn primary" onclick={saveText}>Save decklist</button>
    {/snippet}
  </Modal>
{:else if dialog === 'rename' || dialog === 'duplicate'}
  <Modal title={dialog === 'rename' ? 'Rename deck' : 'Duplicate deck'} onclose={() => (dialog = null)}>
    <label class="field"><span>Deck name</span><input class="input" bind:value={input} /></label>
    {#snippet footer()}
      <button class="btn ghost" onclick={() => (dialog = null)}>Cancel</button>
      <button class="btn primary" disabled={!input.trim()} onclick={() => rename(dialog === 'duplicate')}>
        {dialog === 'rename' ? 'Rename' : 'Duplicate'}
      </button>
    {/snippet}
  </Modal>
{:else if dialog === 'delete'}
  <Modal title="Delete {name}?" onclose={() => (dialog = null)}>
    <p class="dim">The decklist, its game plan, sources and agent history are removed. This can't be undone.</p>
    {#snippet footer()}
      <button class="btn ghost" onclick={() => (dialog = null)}>Keep it</button>
      <button class="btn primary" style="background:var(--danger);border-color:var(--danger)" onclick={remove}>Delete deck</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .hero {
    position: relative;
    display: grid;
    min-height: 300px;
    margin: calc(-1 * clamp(1rem, 3vw, 2.25rem));
    margin-bottom: 0;
    align-items: end;
    z-index: 20;
  }
  .hero.skeleton {
    background: var(--panel);
  }
  .hero-art {
    position: absolute;
    inset: 0;
    overflow: hidden;
    background: var(--panel) center 25% / cover no-repeat;
  }
  .hero-art::after {
    content: '';
    position: absolute;
    inset: 0;
    background:
      linear-gradient(to top, var(--ink) 4%, color-mix(in srgb, var(--ink) 55%, transparent) 55%, transparent),
      linear-gradient(to right, color-mix(in srgb, var(--ink) 80%, transparent), transparent 70%);
  }
  .hero-body {
    position: relative;
    display: grid;
    gap: 0.4rem;
    padding: calc(3.5rem + var(--safe-top)) clamp(1rem, 3vw, 2.25rem) 1.25rem;
    max-width: 60rem;
  }
  .back {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    min-height: 40px;
    margin: -0.5rem 0;
    color: var(--text-2);
    text-decoration: none;
    width: fit-content;
  }
  h1 {
    font-size: clamp(2rem, 4.5vw, 3.4rem);
    text-shadow: 0 2px 18px rgba(0, 0, 0, 0.5);
  }
  .facts {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.9rem;
    color: var(--text-2);
    margin-top: 0.2rem;
  }
  .warn {
    color: var(--danger);
  }
  .actions {
    margin-top: 0.7rem;
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + 6px);
    z-index: 30;
    display: grid;
    min-width: 190px;
    padding: 0.3rem;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0.5rem 0.65rem;
    border: 0;
    background: none;
    border-radius: var(--r-sm);
    cursor: pointer;
    text-align: left;
  }
  .menu button:hover {
    background: var(--panel-2);
  }
  .menu .danger {
    color: var(--danger);
  }

  .tabs {
    position: sticky;
    top: 0;
    z-index: 15;
    display: flex;
    gap: 0.2rem;
    overflow-x: auto;
    scrollbar-width: none;
    margin: 0 calc(-1 * clamp(1rem, 3vw, 2.25rem)) 1.5rem;
    padding: 0 clamp(1rem, 3vw, 2.25rem);
    background: var(--ink);
    border-bottom: 1px solid var(--line-soft);
  }
  .tabs a {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.85rem 0.85rem 0.75rem;
    color: var(--muted);
    text-decoration: none;
    font-weight: 700;
    font-size: 0.92rem;
    border-bottom: 2px solid transparent;
    white-space: nowrap;
  }
  .tabs a:hover {
    color: var(--text);
  }
  .tabs a[aria-current='page'] {
    color: var(--brass-strong);
    border-bottom-color: var(--brass);
  }
  textarea {
    font-size: 0.9rem;
  }
  @media (max-width: 760px) {
    .hero {
      min-height: 250px;
    }
    .hero-body {
      padding-top: 3rem;
    }
  }
  .badge.format {
    cursor: pointer;
    border: 1px solid var(--brass);
    background: transparent;
    color: var(--brass-strong);
    font: inherit;
    font-size: 0.78rem;
    font-weight: 700;
  }
  .format-pick {
    display: grid;
    gap: 0.2rem;
    text-align: left;
    padding: 0.75rem 1rem;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--panel);
    cursor: pointer;
  }
  .format-pick.on {
    border-color: var(--brass);
    background: color-mix(in srgb, var(--brass) 10%, var(--panel));
  }
  .problems ul {
    margin: 0.3rem 0 0;
    padding-left: 1.1rem;
    color: var(--danger);
  }
  .export-tabs {
    flex-wrap: wrap;
    justify-self: start;
  }
  .ok-line {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--ok);
    font-weight: 700;
  }
</style>
