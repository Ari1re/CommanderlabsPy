<!--
The Build screen: generate a new deck. Pick a format, a commander (or
colors for Canadian Highlander), play styles, bracket/power and an optional
budget, then `generate` runs as a job and opens the new deck.
The engine (deep learning or peer-based) comes from Settings → Deck
generation. "Let the AI model help" appears only when an AI model is set
up; "Deep card search" only for peer-based (deep learning always does it).

Things to work on:
- The build log is shown raw; the deck's Sources tab report is easier to
  read, so link to it from the finished toast.
-->
<script lang="ts">
  import { api, runJob, type Job } from '../lib/api';
  import { app, go } from '../lib/store.svelte';
  import type { CardView } from '../lib/types';
  import { artCrop, BRACKETS, FORMATS, POWER_LEVELS, symbolUrl, type FormatId } from '../lib/util';
  import CommanderPicker from '../components/CommanderPicker.svelte';
  import Identity from '../components/Identity.svelte';
  import Icon from '../components/Icon.svelte';
  import CardInput from '../components/CardInput.svelte';

  interface Theme {
    name: string;
    score: number;
    reason: string;
  }

  /** One entry of EDHREC's full theme/typal list (the `archetypes` call). */
  interface Archetype {
    name: string;
    deck_count: number;
    typal: boolean;
  }

  let format = $state<FormatId>('commander');
  let deep = $state(false);
  let aiAssist = $state(false);
  /** Your requests: pet cards, combos to include, cards to leave out, instructions. */
  let include = $state<string[]>([]);
  let combos = $state<string[][]>([]);
  let comboDraft = $state<string[]>([]);
  let exclude = $state<string[]>([]);
  let instructions = $state('');
  let requestCount = $derived(include.length + combos.length + exclude.length + (instructions.trim() ? 1 : 0));

  function addUnique(list: string[], name: string) {
    return list.some((n) => n.toLowerCase() === name.toLowerCase()) ? list : [...list, name];
  }
  /** An AI model the generator can ask: a downloaded built-in model, a running server, or an online API key. */
  let aiReady = $state(false);
  api<any>('ai_status')
    .then((ai) => {
      aiReady =
        ai.running ||
        (ai.uses_built_in && ai.models?.some((m: any) => m.installed)) ||
        (ai.config?.provider === 'api' && ai.config?.has_api_key);
      aiAssist = aiReady;
    })
    .catch(() => (aiReady = false));
  let colors = $state<string[]>([]);
  let customTheme = $state('');
  let commanders = $state<CardView[]>([]);
  let themes = $state<Theme[] | null>(null);
  /** Selected play styles, in the order picked. The first is the main one. */
  let picked = $state<string[]>([]);
  const MAX_THEMES = 3;
  let bracket = $state(3);
  /** Budget: total for the deck ('' = no limit) and an optional per-card cap. */
  const BUDGETS = [50, 100, 250, 500];
  let budgetTotal = $state<number | null>(null);
  let customTotal = $state('');
  let showCustom = $state(false);
  let perCard = $state('');
  let total = $derived(showCustom ? (Number(customTotal) > 0 ? Number(customTotal) : null) : budgetTotal);
  let cardCap = $derived(Number(perCard) > 0 ? Number(perCard) : null);
  let job = $state<Job | null>(null);
  let failed = $state<string | null>(null);

  let spec = $derived(commanders.map((c) => c.name).join(' + '));
  let formatInfo = $derived(FORMATS.find((f) => f.id === format)!);
  let powerLevels = $derived(format === 'commander' ? BRACKETS : POWER_LEVELS[format]);
  let colorSpec = $derived(['W', 'U', 'B', 'R', 'G'].filter((c) => colors.includes(c)).join(''));
  let ready = $derived(format === 'canlander' ? true : commanders.length > 0);
  const COLOR_NAMES: Record<string, string> = { W: 'White', U: 'Blue', B: 'Black', R: 'Red', G: 'Green' };

  function setFormat(f: FormatId) {
    if (f === format) return;
    format = f;
    commanders = [];
    themes = null;
    picked = [];
    failed = null;
    if (f === 'canlander') loadThemes();
  }

  function toggleColor(c: string) {
    colors = colors.includes(c) ? colors.filter((x) => x !== c) : [...colors, c];
  }

  // Every archetype EDHREC tracks, loaded the first time the list is opened
  // or the custom box is focused. Filtered by what's typed in that box.
  let archetypes = $state<Archetype[] | null>(null);
  let browsing = $state(false);
  let browseFilter = $derived(customTheme.trim().toLowerCase());
  let browseList = $derived(
    (archetypes ?? []).filter((a) => !browseFilter || a.name.toLowerCase().includes(browseFilter)),
  );

  async function loadArchetypes() {
    if (archetypes) return;
    archetypes = [];
    try {
      archetypes = (await api<{ archetypes: Archetype[] }>('archetypes')).archetypes;
    } catch (err) {
      app.error(err);
      archetypes = null;
      browsing = false;
    }
  }

  function toggleBrowse() {
    browsing = !browsing;
    if (browsing) loadArchetypes();
  }

  function archetypeReason(a: Archetype) {
    const kind = a.typal ? 'EDHREC typal' : 'EDHREC theme';
    return a.deck_count ? `${kind} • ${a.deck_count.toLocaleString()} deck(s) site-wide` : kind;
  }

  function addCustomTheme() {
    const name = customTheme.trim();
    if (!name) return;
    const known = archetypes?.find((a) => a.name.toLowerCase() === name.toLowerCase());
    addTheme(known?.name ?? name, known ? archetypeReason(known) : `Your own play style: cards whose rules text matches "${name}".`);
    customTheme = '';
  }

  /** Add a play style that isn't in the commander's suggestions and pick it. */
  function addTheme(name: string, reason: string) {
    if (!themes) themes = [];
    if (!themes.some((t) => t.name.toLowerCase() === name.toLowerCase())) {
      themes = [{ name, score: 0, reason }, ...themes];
    }
    if (!picked.some((t) => t.toLowerCase() === name.toLowerCase())) {
      if (picked.length >= MAX_THEMES) picked = picked.slice(0, MAX_THEMES - 1);
      picked = [...picked, name];
    }
    deep = true;
  }
  let canPair = $derived(
    commanders.length === 1 &&
      /partner|friends forever|choose a background|doctor's companion/i.test(
        commanders[0].oracle_text + ' ' + commanders[0].type_line,
      ),
  );

  async function loadThemes() {
    themes = null;
    picked = [];
    if (format !== 'canlander' && !commanders.length) return;
    try {
      const res = await api<{ themes: Theme[] }>('generation_themes', { commander: spec, format });
      themes = res.themes;
      picked = themes[0] && format !== 'canlander' ? [themes[0].name] : [];
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

  function toggleTheme(name: string) {
    if (picked.includes(name)) {
      picked = picked.filter((t) => t !== name);
    } else if (picked.length < MAX_THEMES) {
      picked = [...picked, name];
    } else {
      app.toast(`Pick up to ${MAX_THEMES} play styles. Tap one to remove it first.`);
    }
  }

  async function build() {
    failed = null;
    job = { id: 0, kind: 'generate', subject: spec, status: 'running', percent: 0, step: 'Starting', detail: '', log: [], result: null, error: null };
    try {
      const res = await runJob<{ name: string }>(
        'generate',
        {
          commander: spec,
          theme: picked.join(' + '),
          bracket,
          format,
          deep_search: deep,
          ai_assist: aiReady && aiAssist && format !== 'canlander',
          colors: colorSpec,
          budget_total: total,
          budget_card: cardCap,
          requests: { include, combos, exclude, instructions },
        },
        (j) => (job = j),
        800,
      );
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
    Pick a format, a commander (or colors for Canadian Highlander), a power level and a play style.
    Commander Lab researches how it's usually built, then assembles a deck that follows the format's rules.
  </p>
  <div class="segmented formats" role="radiogroup" aria-label="Format">
    {#each FORMATS as f (f.id)}
      <button role="radio" aria-checked={format === f.id} onclick={() => setFormat(f.id)}>{f.label}</button>
    {/each}
  </div>
  <p class="small dim">{formatInfo.summary}</p>
</header>

{#if job && !failed}
  <section class="building panel">
    {#if commanders[0]}
      <div class="building-art" style:background-image={`url("${artCrop(commanders[0].image_uri)}")`}></div>
    {/if}
    <div class="building-body">
      <h2>Building {format === 'canlander' ? `a Canadian Highlander deck` : spec}</h2>
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
      {#if format === 'canlander'}
      <h2>Colors</h2>
      <div class="colors" role="group" aria-label="Deck colors">
        {#each ['W', 'U', 'B', 'R', 'G'] as c (c)}
          <button class="color" aria-pressed={colors.includes(c)} onclick={() => toggleColor(c)} aria-label={COLOR_NAMES[c]} title={COLOR_NAMES[c]}>
            <img src={symbolUrl(c)} alt="" width="34" height="34" />
          </button>
        {/each}
      </div>
      <p class="small muted">
        {colors.length ? `${colors.map((c) => COLOR_NAMES[c]).join(' / ')}.` : 'No colors picked: a colorless deck.'}
        There's no commander; you play one-on-one at 20 life.
      </p>
      {:else}
      <h2>{format === 'pdh' ? 'Commander (an uncommon creature)' : 'Commander'}</h2>
      {#if commanders.length < 2}
        <CommanderPicker
          {format}
          label={commanders.length ? 'Partner or Background' : 'Search for a commander'}
          placeholder={commanders.length ? 'Add a partner (optional)' : format === 'pdh' ? 'Murmuring Mystic' : 'Kefka, Court Mage'}
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
      {/if}
    </section>

    <section class="stack">
      <h2>Power level</h2>
      {#if format !== 'commander'}
        <p class="small muted">Commander brackets don't apply to {formatInfo.label}; this sets how hard the build pushes.</p>
      {/if}
      <div class="brackets" role="radiogroup" aria-label="Power level">
        {#each powerLevels as b (b.n)}
          <button role="radio" aria-checked={bracket === b.n} class="bracket" onclick={() => (bracket = b.n)}>
            <span class="b-num">{b.n}</span>
            <span class="b-name">{b.name}</span>
            <span class="b-blurb small dim">{b.blurb}</span>
          </button>
        {/each}
      </div>

      <div class="label-row">
        <h2>Play style</h2>
        {#if themes?.length}<span class="small muted">Pick up to {MAX_THEMES}. The first is the main focus.</span>{/if}
      </div>
      {#if format !== 'canlander' && !commanders.length}
        <p class="muted">Choose a commander to see its most popular play styles.</p>
      {:else if themes === null}
        <p class="muted">Looking up play styles…</p>
      {:else if themes.length === 0}
        <p class="muted">No themes found; the deck will follow the commander's own synergy.</p>
      {:else}
        <div class="themes">
          {#each themes as t (t.name)}
            {@const order = picked.indexOf(t.name)}
            <button
              class="theme"
              aria-pressed={order >= 0}
              disabled={order < 0 && picked.length >= MAX_THEMES}
              onclick={() => toggleTheme(t.name)}
              title={t.reason}
            >
              {#if order >= 0}<span class="order num">{order + 1}</span>{/if}
              {t.name}
            </button>
          {/each}
        </div>
        {#if picked.length}
          <p class="small dim">
            {#if picked.length === 1}
              {themes.find((t) => t.name === picked[0])?.reason}
            {:else}
              Built around {picked[0]}, blended with {picked.slice(1).join(' and ')}.
            {/if}
          </p>
        {:else}
          <p class="small muted">No style picked: the deck follows the commander's usual build.</p>
        {/if}
      {/if}

      {#if format === 'canlander' || commanders.length}
        <form class="custom" onsubmit={(e) => { e.preventDefault(); addCustomTheme(); }}>
          <input
            class="input"
            bind:value={customTheme}
            onfocus={loadArchetypes}
            list="archetype-list"
            placeholder="Add your own (e.g. -1/-1 Counters, Elves, Landfall)"
            aria-label="Add your own play style"
          />
          <button class="btn" disabled={!customTheme.trim()}>Add</button>
        </form>
        <datalist id="archetype-list">
          {#each archetypes ?? [] as a (a.name)}<option value={a.name}></option>{/each}
        </datalist>
        <button class="btn ghost browse" onclick={toggleBrowse} aria-expanded={browsing}>
          {browsing ? 'Hide' : 'Browse'} all EDHREC play styles{archetypes?.length ? ` (${archetypes.length})` : ''}
        </button>
        {#if browsing}
          <!-- Full EDHREC list, themes then typal, filtered by the box above. -->
          {#if !archetypes?.length}
            <p class="muted small">Loading EDHREC's play styles…</p>
          {:else if browseList.length === 0}
            <p class="muted small">No EDHREC play style matches "{customTheme.trim()}". Add it as your own.</p>
          {:else}
            {#each [{ label: 'Themes', typal: false }, { label: 'Typal', typal: true }] as group (group.label)}
              {@const list = browseList.filter((a) => a.typal === group.typal)}
              {#if list.length}
                <h3 class="small muted browse-head">{group.label} · {list.length}</h3>
                <div class="themes browse-list">
                  {#each list as a (a.name)}
                    {@const on = picked.some((p) => p.toLowerCase() === a.name.toLowerCase())}
                    <button
                      class="theme"
                      aria-pressed={on}
                      disabled={!on && picked.length >= MAX_THEMES}
                      onclick={() => (on ? toggleTheme(picked.find((p) => p.toLowerCase() === a.name.toLowerCase())!) : addTheme(a.name, archetypeReason(a)))}
                      title={archetypeReason(a)}
                    >
                      {a.name}
                      {#if a.deck_count}<span class="small dim num">{a.deck_count.toLocaleString()}</span>{/if}
                    </button>
                  {/each}
                </div>
              {/if}
            {/each}
          {/if}
        {/if}
      {/if}

      <div class="label-row">
        <h2>Budget</h2>
        <span class="small muted">Prices in US dollars, from Scryfall (TCGplayer).</span>
      </div>
      <div class="budget stack">
        <div class="segmented wrap" role="group" aria-label="Deck budget">
          <button aria-pressed={!showCustom && budgetTotal === null} onclick={() => { showCustom = false; budgetTotal = null; }}>No limit</button>
          {#each BUDGETS as b (b)}
            <button aria-pressed={!showCustom && budgetTotal === b} onclick={() => { showCustom = false; budgetTotal = b; }}>${b}</button>
          {/each}
          <button aria-pressed={showCustom} onclick={() => (showCustom = true)}>Custom</button>
        </div>
        <div class="budget-inputs">
          {#if showCustom}
            <label class="money">
              <span class="small">Whole deck</span>
              <span class="input-wrap"><span class="dim">$</span><input class="input" type="number" inputmode="decimal" min="1" step="1" bind:value={customTotal} placeholder="150" /></span>
            </label>
          {/if}
          <label class="money">
            <span class="small">Most per card <span class="dim">(optional)</span></span>
            <span class="input-wrap"><span class="dim">$</span><input class="input" type="number" inputmode="decimal" min="0.1" step="0.5" bind:value={perCard} placeholder="Any" /></span>
          </label>
        </div>
        {#if total || cardCap}
          <p class="small muted">
            {#if total}The whole deck, commander included, stays under ${total}: pricey cards are left out, and the most expensive picks are swapped for cheaper ones that do the same job.{/if}
            {#if cardCap}No card over ${cardCap}.{/if}
            The commander you picked always stays.
          </p>
        {/if}
      </div>

      {#if app.generation === 'deep' && format !== 'canlander'}
        <p class="small dim">
          Built with deep learning generation (Settings → Deck generation): rules-text search is always on, and you get a
          report of every source and why each card is in.
          {#if !aiReady}For better picks, set up an AI model in Settings → AI model.{/if}
        </p>
      {:else}
      <label class="deep panel">
        <input type="checkbox" bind:checked={deep} />
        <span>
          <b>Deep card search</b>
          <span class="small dim">
            Reads the rules text of every card in Scryfall's database for cards that fit your play styles,
            including older cards no decklist runs. For example, wither creatures for a -1/-1 counters deck.
          </span>
        </span>
      </label>
      {/if}

      <details class="requests panel" open={requestCount > 0}>
        <summary>
          <b>Your requests</b>
          <span class="small dim">{requestCount ? `${requestCount} added` : 'Pet cards, combos, cards to leave out, instructions'}</span>
        </summary>
        <div class="stack">
          <div class="req">
            <span class="small"><b>Always include</b> <span class="dim">your pet cards; they're added first and kept through every later step</span></span>
            <CardInput label="Card to include" placeholder="Search a card to include" onpick={(n) => (include = addUnique(include, n))} />
            {#if include.length}
              <div class="req-chips">
                {#each include as n (n)}<button class="req-chip" onclick={() => (include = include.filter((x) => x !== n))} aria-label="Remove {n}">{n} ×</button>{/each}
              </div>
            {/if}
          </div>

          {#if format !== 'canlander'}
            <div class="req">
              <span class="small"><b>Combos to include</b> <span class="dim">pick each card, then add the combo</span></span>
              <div class="row combo-row">
                <CardInput label="Combo card" placeholder={comboDraft.length ? 'Next card in the combo' : 'First card in the combo'} onpick={(n) => (comboDraft = addUnique(comboDraft, n))} />
                <button class="btn" disabled={comboDraft.length < 2} onclick={() => { combos = [...combos, comboDraft]; comboDraft = []; }}>Add combo</button>
              </div>
              {#if comboDraft.length}
                <div class="req-chips">
                  <span class="small dim">New combo:</span>
                  {#each comboDraft as n, i (n)}{#if i > 0}<span class="dim">+</span>{/if}<button class="req-chip" onclick={() => (comboDraft = comboDraft.filter((x) => x !== n))}>{n} ×</button>{/each}
                </div>
              {/if}
              {#if combos.length}
                <div class="req-chips">
                  {#each combos as c, i (i)}<button class="req-chip combo" onclick={() => (combos = combos.filter((_, j) => j !== i))} aria-label="Remove combo {c.join(' + ')}">{c.join(' + ')} ×</button>{/each}
                </div>
              {/if}
            </div>
          {/if}

          <div class="req">
            <span class="small"><b>Leave out</b> <span class="dim">cards you never want in this deck</span></span>
            <CardInput label="Card to leave out" placeholder="Search a card to leave out" onpick={(n) => (exclude = addUnique(exclude, n))} />
            {#if exclude.length}
              <div class="req-chips">
                {#each exclude as n (n)}<button class="req-chip out" onclick={() => (exclude = exclude.filter((x) => x !== n))} aria-label="Remove {n}">{n} ×</button>{/each}
              </div>
            {/if}
          </div>

          <label class="req">
            <span class="small"><b>Instructions</b>
              <span class="dim">{aiReady && format !== 'canlander' ? 'followed by the AI model when "Let the AI model help" is on, and kept in the deck notes' : 'kept in the deck notes; set up an AI model in Settings to have it follow them'}</span>
            </span>
            <textarea class="input" rows="3" bind:value={instructions} placeholder="Lean on tokens over counters. No infinite combos. Keep lots of instant-speed interaction."></textarea>
          </label>
          <p class="small muted">Requested cards must be legal and in your colors; anything that can't go in is explained in the deck notes. They're included even over the per-card budget.</p>
        </div>
      </details>

      {#if aiReady && format !== 'canlander'}
        <label class="deep panel">
          <input type="checkbox" bind:checked={aiAssist} />
          <span>
            <b>Let the AI model help</b>{#if app.generation === 'deep'} <span class="badge brass">Recommended</span>{/if}
            <span class="small dim">
              Before building, asks your AI model for play styles (when you haven't picked any), key cards for this
              commander, and cards to avoid. Its picks still have to be real, legal cards in your colors, and they're a
              nudge, not a takeover. Everything it suggested shows in the build log.
            </span>
          </span>
        </label>
      {/if}

      {#if failed}
        <p class="error">{failed}</p>
      {/if}

      <div>
        <button class="btn primary big" disabled={!ready || !app.status?.card_data_ready} onclick={build}>
          <Icon name="build" size={18} /> Build the deck
        </button>
      </div>
      <p class="small muted">
        {#if format === 'pdh'}
          Every deck is checked before it's saved: exactly 100 cards, singleton, inside the commander's colors,
          an uncommon creature commander and 99 commons, nothing on the PDH banned list.
        {:else if format === 'canlander'}
          Every deck is checked before it's saved: 100 cards, singleton, Vintage-legal and within the 10-point limit.
        {:else}
          Every deck is checked before it's saved: exactly 100 cards, singleton, inside the
          commander's colors, Commander-legal, and locked to the bracket you picked.
        {/if}
      </p>
    </section>
  </div>
{/if}

<style>
  .requests {
    padding: 0.8rem 1rem;
  }
  .requests summary {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.5rem;
    cursor: pointer;
  }
  .requests .stack {
    margin-top: 0.75rem;
  }
  .req {
    display: grid;
    gap: 0.4rem;
  }
  .combo-row {
    flex-wrap: nowrap;
    gap: 0.5rem;
  }
  .req-chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem;
  }
  .req-chip {
    padding: 0.25rem 0.65rem;
    border-radius: 99px;
    border: 1px solid var(--brass);
    background: var(--panel-2);
    color: var(--text);
    cursor: pointer;
    font: inherit;
    font-size: 0.85rem;
    font-weight: 700;
  }
  .req-chip.out {
    border-color: var(--danger);
  }
  .req-chip.combo {
    border-style: dashed;
  }
  .head {
    display: grid;
    gap: 0.5rem;
    margin-bottom: 2rem;
  }
  .lede {
    max-width: 60ch;
  }
  .formats {
    justify-self: start;
    flex-wrap: wrap;
  }
  .formats button {
    min-height: 40px;
  }
  .colors {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .color {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    border-radius: 50%;
    border: 2px solid var(--line);
    background: var(--panel);
    cursor: pointer;
    opacity: 0.55;
  }
  .color[aria-pressed='true'] {
    border-color: var(--brass);
    opacity: 1;
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--brass) 30%, transparent);
  }
  .custom {
    display: flex;
    gap: 0.5rem;
  }
  .custom .input {
    flex: 1;
    min-width: 0;
  }
  .browse {
    align-self: flex-start;
  }
  .browse-head {
    margin: 0.25rem 0 0;
  }
  .browse-list {
    max-height: 18rem;
    overflow-y: auto;
  }
  .segmented.wrap {
    flex-wrap: wrap;
  }
  .budget-inputs {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
  }
  .money {
    display: grid;
    gap: 0.25rem;
  }
  .input-wrap {
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }
  .input-wrap .input {
    width: 8rem;
  }
  .deep {
    display: flex;
    gap: 0.75rem;
    align-items: flex-start;
    padding: 0.8rem 1rem;
    cursor: pointer;
  }
  .deep input {
    accent-color: var(--brass);
    width: 20px;
    height: 20px;
    margin-top: 0.15rem;
    flex-shrink: 0;
  }
  .deep span {
    display: grid;
    gap: 0.2rem;
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(260px, 340px) minmax(0, 1fr);
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
    grid-template-columns: repeat(5, minmax(0, 1fr));
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
  .theme:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .theme {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    min-height: 36px;
  }
  .order {
    display: inline-grid;
    place-items: center;
    width: 1.2rem;
    height: 1.2rem;
    border-radius: 50%;
    background: var(--brass);
    color: var(--brass-ink);
    font-size: 0.72rem;
  }
  .label-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.75rem;
    flex-wrap: wrap;
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
    grid-template-columns: 320px minmax(0, 1fr);
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
      grid-template-columns: minmax(0, 1fr);
      gap: 2rem;
    }
    .brackets {
      grid-template-columns: minmax(0, 1fr);
    }
    .bracket {
      grid-template-columns: 2.2rem auto minmax(0, 1fr);
      align-items: baseline;
      gap: 0.6rem;
    }
    .building {
      grid-template-columns: minmax(0, 1fr);
    }
    .building-art {
      min-height: 160px;
    }
    figure {
      width: 46%;
    }
  }
</style>
