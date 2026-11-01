<script lang="ts">
  import { api, runJob } from '../../lib/api';
  import { go } from '../../lib/store.svelte';
  import { app } from '../../lib/store.svelte';
  import type { Analysis, CardView, Deck, Simulation } from '../../lib/types';
  import { BRACKETS, MANA, MANA_NAME } from '../../lib/util';
  import Icon from '../../components/Icon.svelte';

  let {
    name,
    analysis,
    onchange,
  }: { name: string; analysis: Analysis | null; onchange: (d: Deck) => void } = $props();

  let sim = $state<Simulation | null>(null);
  let simBusy = $state(false);

  async function simulate() {
    simBusy = true;
    try {
      sim = await api<Simulation>('simulate', { name, runs: 50000 });
    } catch (err) {
      app.error(err);
    } finally {
      simBusy = false;
    }
  }

  let speed = $state<any>(null);
  let speedBusy = $state('');
  let upTarget = $state(4);
  let upgrading = $state<{ percent: number; step: string } | null>(null);

  async function winSpeed() {
    speedBusy = 'Starting';
    try {
      speed = await runJob('win_speed', { name }, (j) => (speedBusy = j.detail || j.step), 800);
    } catch (err) {
      app.error(err);
    } finally {
      speedBusy = '';
    }
  }

  async function upgrade() {
    upgrading = { percent: 0, step: 'Starting' };
    try {
      const res = await runJob<{ name: string }>('upgrade', { name, bracket: upTarget }, (j) => (upgrading = { percent: j.percent, step: j.step }), 800);
      app.toast(`Saved ${res.name}.`, 'success');
      go({ page: 'deck', name: res.name, tab: 'cards' });
    } catch (err) {
      app.error(err);
    } finally {
      upgrading = null;
    }
  }

  async function apply(args: { cut?: string; add?: string }, message: string) {
    try {
      onchange(await api<Deck>('apply_suggestion', { name, ...args }));
      app.toast(message, 'success');
    } catch (err) {
      app.error(err);
    }
  }

  async function inspect(card: string) {
    try {
      app.open(await api<CardView>('card', { name: card }));
    } catch (err) {
      app.error(err);
    }
  }

  const roleLabels: [string, string][] = [
    ['ramp', 'Ramp'],
    ['draw', 'Card draw'],
    ['removal', 'Removal'],
    ['counters', 'Counterspells'],
    ['wipes', 'Board wipes'],
    ['tutors', 'Tutors'],
    ['protection', 'Protection'],
    ['recursion', 'Recursion'],
  ];
  const roleTargets: Record<string, number> = { ramp: 10, draw: 10, removal: 8, counters: 0, wipes: 3, tutors: 0, protection: 3, recursion: 2 };

  let curveMax = $derived(Math.max(1, ...(analysis?.curve ?? [1])));
  let manaMax = $derived(Math.max(1, ...(analysis?.pip_demand ?? [1]), ...(analysis?.direct_sources ?? [1])));
  let types = $derived(
    analysis
      ? Object.entries(analysis.types)
          .filter(([, n]) => n > 0)
          .sort((a, b) => b[1] - a[1])
      : [],
  );
</script>

{#if !analysis}
  <p class="muted">Analyzing… If this doesn't finish, download the card database from Settings.</p>
{:else}
  <div class="summary">
    <div><b class="num">{analysis.avg_cmc.toFixed(2)}</b><span>Average mana value</span></div>
    <div><b class="num">{analysis.types.lands ?? 0}</b><span>Lands</span></div>
    <div><b class="num">{analysis.roles.ramp}</b><span>Ramp</span></div>
    <div><b class="num">{analysis.roles.draw}</b><span>Card draw</span></div>
    <div><b class="num">{analysis.roles.removal + analysis.roles.counters + analysis.roles.wipes}</b><span>Interaction</span></div>
  </div>

  <div class="grid">
    <section class="panel box">
      <h3>Mana curve</h3>
      <div class="curve" role="img" aria-label="Mana curve: {analysis.curve.join(', ')}">
        {#each analysis.curve as n, i (i)}
          <div class="col">
            <span class="num small">{n || ''}</span>
            <i style:height="{(n / curveMax) * 100}%"></i>
            <span class="small muted">{i === 7 ? '7+' : i}</span>
          </div>
        {/each}
      </div>
    </section>

    <section class="panel box">
      <h3>Colors: sources vs. demand</h3>
      <div class="mana">
        {#each MANA as c, i (c)}
          {#if analysis.pip_demand[i] || analysis.direct_sources[i]}
            <div class="mana-row">
              <span class="pip" style:background="var(--{c.toLowerCase()})" title={MANA_NAME[c]}></span>
              <div class="bars">
                <i class="src" style:width="{(analysis.direct_sources[i] / manaMax) * 100}%" style:background="var(--{c.toLowerCase()})"></i>
                <i class="dem" style:width="{(analysis.pip_demand[i] / manaMax) * 100}%"></i>
              </div>
              <span class="small num dim">{analysis.direct_sources[i]} / {analysis.pip_demand[i]}</span>
            </div>
          {/if}
        {/each}
        <p class="small muted">Top bar: lands and rocks that make the color. Bottom bar: colored symbols in your spells.</p>
      </div>
    </section>

    <section class="panel box">
      <h3>Roles</h3>
      <div class="roles">
        {#each roleLabels as [key, label] (key)}
          {@const n = analysis.roles[key] ?? 0}
          <div class="role">
            <span>{label}</span>
            <div class="track"><i style:width="{Math.min(100, (n / 14) * 100)}%" class:low={roleTargets[key] && n < roleTargets[key] * 0.7}></i></div>
            <span class="num">{n}</span>
          </div>
        {/each}
      </div>
    </section>

    <section class="panel box">
      <h3>Card types</h3>
      <ul class="types">
        {#each types as [t, n] (t)}
          <li><span>{t[0].toUpperCase() + t.slice(1)}</span><span class="num">{n}</span></li>
        {/each}
      </ul>
      {#if analysis.tags.length}
        <h3 class="mt">Detected themes</h3>
        <div class="row">
          {#each analysis.tags.slice(0, 8) as tag (tag.name)}
            <span class="badge" title="{tag.matched_cards} cards">{tag.name}</span>
          {/each}
        </div>
      {/if}
    </section>
  </div>

  <section class="panel box bracket">
    <div>
      <span class="b-num">{analysis.bracket.bracket}</span>
    </div>
    <div class="stack">
      <h3>Bracket {analysis.bracket.bracket}: {analysis.bracket.label}</h3>
      <ul class="reasons">
        {#each analysis.bracket.reasons as r, i (i)}<li>{r}</li>{/each}
      </ul>
      {#if analysis.bracket.game_changers.length}
        <p class="small">
          <span class="dim">Game Changers:</span>
          {#each analysis.bracket.game_changers as gc, i (gc)}<button class="link" onclick={() => inspect(gc)}>{gc}</button>{i < analysis.bracket.game_changers.length - 1 ? ', ' : ''}{/each}
        </p>
      {/if}
      <p class="small muted">{analysis.bracket.confidence_note} Run Combos to refine this with Commander Spellbook.</p>
    </div>
  </section>

  {#if !analysis.legal || analysis.unknown_cards.length}
    <section class="panel box issues">
      <h3>Legality</h3>
      <ul>
        {#each analysis.legality_issues as issue, i (i)}<li>{issue}</li>{/each}
        {#if analysis.unknown_cards.length}
          <li>Not found in the card database: {analysis.unknown_cards.join(', ')}</li>
        {/if}
      </ul>
    </section>
  {/if}

  <div class="grid two">
    <section class="panel box">
      <h3>Suggested cuts</h3>
      <p class="small muted">Weakest fits for your deck's themes and roles.</p>
      <ul class="suggest">
        {#each analysis.cut_candidates.slice(0, 12) as c (c.card)}
          <li>
            <button class="link" onclick={() => inspect(c.card)}>{c.card}</button>
            <span class="small muted why">{c.reason}</span>
            <button class="btn small" onclick={() => apply({ cut: c.card }, `Moved ${c.card} to the sideboard.`)} title="Move to sideboard"><Icon name="sideboard" size={15} /> Cut</button>
          </li>
        {/each}
      </ul>
    </section>
    <section class="panel box">
      <h3>Suggested adds</h3>
      <p class="small muted">Popular cards that fit your themes and colors.</p>
      <ul class="suggest">
        {#each analysis.add_suggestions.slice(0, 12) as a (a.card)}
          <li>
            <button class="link" onclick={() => inspect(a.card)}>{a.card}</button>
            <span class="small muted why">{a.role}{a.edhrec_rank ? ` · EDHREC #${a.edhrec_rank}` : ''}</span>
            <button class="btn small" onclick={() => apply({ add: a.card }, `Added ${a.card}.`)}><Icon name="plus" size={15} /> Add</button>
          </li>
        {/each}
      </ul>
    </section>
  </div>

  <section class="panel box">
    <div class="row">
      <h3>Opening hands and land drops</h3>
      <span class="spacer"></span>
      <button class="btn" onclick={simulate} disabled={simBusy}><Icon name="play" size={15} /> {simBusy ? 'Simulating…' : sim ? 'Run again' : 'Simulate 50,000 games'}</button>
    </div>
    {#if sim}
      <div class="sim">
        {#each [['Keepable opening hands', sim.keepable_opening_pct], ['Ramp by turn 2', sim.early_ramp_by_t2_pct], ['3 lands by turn 3', sim.three_lands_by_t3_pct], ['4 lands by turn 4', sim.four_lands_by_t4_pct], ['5 lands by turn 5', sim.five_lands_by_t5_pct]] as [label, pct] (label)}
          <div class="sim-row">
            <span>{label}</span>
            <div class="track"><i style:width="{pct}%"></i></div>
            <span class="num">{Number(pct).toFixed(0)}%</span>
          </div>
        {/each}
        <p class="small muted">Average lands in the opening seven: {sim.average_opening_lands.toFixed(2)}.</p>
      </div>
    {:else}
      <p class="small muted">Draws thousands of opening hands to show how consistent your mana is.</p>
    {/if}
  </section>

  <section class="panel box">
    <div class="row">
      <h3>How fast does it win?</h3>
      <span class="spacer"></span>
      <button class="btn" onclick={winSpeed} disabled={!!speedBusy}><Icon name="play" size={15} /> {speedBusy ? 'Testing…' : speed ? 'Test again' : 'Goldfish test'}</button>
    </div>
    {#if speedBusy}<p class="small dim">{speedBusy}</p>{/if}
    {#if speed?.goldfish}
      {@const g = speed.goldfish}
      <div class="summary inner">
        <div><b class="num">{g.most_likely_combo_turn ? `T${g.most_likely_combo_turn}` : '—'}</b><span>Most likely win turn</span></div>
        <div><b class="num">{g.median_combo_turn ? `T${g.median_combo_turn}` : '—'}</b><span>Median win turn</span></div>
        <div><b class="num">{g.earliest_combo_turn ? `T${g.earliest_combo_turn}` : '—'}</b><span>Fastest win seen</span></div>
        <div><b class="num">{Math.round(g.keepable_pct)}%</b><span>Keepable hands</span></div>
        <div><b class="num">{g.runs.toLocaleString()}</b><span>Games played</span></div>
      </div>
      <p class="small muted">{g.caveat}</p>
    {:else if !speedBusy}
      <p class="small muted">Plays thousands of solo games with your combo lines to estimate how quickly the deck can win.</p>
    {/if}
  </section>

  <section class="panel box">
    <h3>Rebuild at another bracket</h3>
    <p class="small dim">Keeps the cards that matter to your plan and rebuilds the rest to hit the bracket. Saved as a new deck; this one isn't changed.</p>
    {#if upgrading}
      <div class="progress"><i style:width="{upgrading.percent}%"></i></div>
      <p class="small dim">{upgrading.step}</p>
    {:else}
      <div class="row">
        <select class="input up" bind:value={upTarget}>
          {#each BRACKETS as b (b.n)}<option value={b.n}>Bracket {b.n} · {b.name}</option>{/each}
        </select>
        <button class="btn" onclick={upgrade}><Icon name="build" size={15} /> Rebuild</button>
      </div>
    {/if}
  </section>
{/if}

<style>
  .summary {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 1px;
    background: var(--line-soft);
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    overflow: hidden;
    margin-bottom: 1.25rem;
  }
  .summary > div {
    display: grid;
    gap: 0.1rem;
    padding: 0.9rem 1rem;
    background: var(--panel);
  }
  .summary b {
    font-family: var(--display);
    font-weight: 400;
    font-size: 1.9rem;
    line-height: 1.1;
    color: var(--brass-strong);
  }
  .summary span {
    color: var(--text-2);
    font-size: 0.86rem;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 1.25rem;
    margin-bottom: 1.25rem;
  }
  .box {
    padding: 1.1rem 1.25rem;
    display: grid;
    gap: 0.75rem;
    align-content: start;
  }
  .box + .box,
  .bracket,
  .issues {
    margin-bottom: 1.25rem;
  }
  .mt {
    margin-top: 0.5rem;
  }

  .curve {
    display: grid;
    grid-template-columns: repeat(8, minmax(0, 1fr));
    gap: 0.4rem;
    height: 160px;
    align-items: end;
  }
  .col {
    display: grid;
    grid-template-rows: auto 1fr auto;
    height: 100%;
    justify-items: center;
    gap: 0.25rem;
  }
  .col i {
    align-self: end;
    width: 100%;
    min-height: 2px;
    background: var(--brass);
    border-radius: 3px 3px 0 0;
  }

  .mana {
    display: grid;
    gap: 0.6rem;
  }
  .mana-row {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) 4.5rem;
    gap: 0.6rem;
    align-items: center;
  }
  .pip {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.3);
  }
  .bars {
    display: grid;
    gap: 3px;
  }
  .bars i {
    display: block;
    height: 8px;
    border-radius: 99px;
    min-width: 2px;
  }
  .bars .dem {
    background: var(--muted);
    opacity: 0.6;
  }

  .roles,
  .sim {
    display: grid;
    gap: 0.5rem;
  }
  .role,
  .sim-row {
    display: grid;
    grid-template-columns: 9.5rem minmax(0, 1fr) 2.5rem;
    gap: 0.6rem;
    align-items: center;
  }
  .sim-row {
    grid-template-columns: 12rem minmax(0, 1fr) 3rem;
  }
  .track {
    height: 8px;
    border-radius: 99px;
    background: var(--line-soft);
    overflow: hidden;
  }
  .track i {
    display: block;
    height: 100%;
    background: var(--brass);
  }
  .track i.low {
    background: var(--danger);
  }
  .types {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.25rem;
  }
  .types li {
    display: flex;
    justify-content: space-between;
    border-bottom: 1px dashed var(--line-soft);
  }

  .bracket {
    grid-template-columns: auto minmax(0, 1fr);
    gap: 1.5rem;
  }
  .b-num {
    font-family: var(--display);
    font-size: 5rem;
    line-height: 0.9;
    color: var(--brass-strong);
  }
  .reasons,
  .issues ul {
    margin: 0;
    padding-left: 1.1rem;
    display: grid;
    gap: 0.2rem;
  }
  .issues {
    border-color: var(--danger);
  }

  .suggest {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
  }
  .suggest li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    grid-template-areas: 'name btn' 'why btn';
    column-gap: 0.75rem;
    padding: 0.45rem 0;
    border-bottom: 1px solid var(--line-soft);
  }
  .suggest .link {
    grid-area: name;
    justify-self: start;
  }
  .why {
    grid-area: why;
  }
  .suggest .btn {
    grid-area: btn;
    align-self: center;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    cursor: pointer;
    text-align: left;
  }
  .link:hover {
    color: var(--brass-strong);
    text-decoration: underline;
  }
  .summary.inner {
    margin: 0;
  }
  .up {
    width: auto;
  }
  .box + .box {
    margin-top: 1.25rem;
  }
  .two {
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  }

  @media (max-width: 760px) {
    .summary {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .summary > div:last-child {
      grid-column: span 2;
    }
    .role,
    .sim-row {
      grid-template-columns: 7.5rem minmax(0, 1fr) 2.6rem;
    }
    .b-num {
      font-size: 3.5rem;
    }
  }
</style>
