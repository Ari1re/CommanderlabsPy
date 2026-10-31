<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { api } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { game, newPlayer, PLAYER_COLORS, type Player } from '../lib/game.svelte';
  import type { DeckSummary } from '../lib/types';
  import Icon from '../components/Icon.svelte';
  import PlayerPanel from '../components/game/PlayerPanel.svelte';
  import CountersSheet from '../components/game/CountersSheet.svelte';

  // --- Setup --------------------------------------------------------------
  const saved = game.state;
  let count = $state(saved?.players.length ?? 4);
  let startLife = $state(saved?.startLife ?? 40);
  let customLife = $state('');
  let tableMode = $state(saved?.tableMode ?? true);
  let cmdLowersLife = $state(saved?.cmdLowersLife ?? true);
  let players = $state<Player[]>(
    saved?.players.map((p) => ({ ...p })) ?? Array.from({ length: 6 }, (_, i) => newPlayer(i, 40)),
  );
  while (players.length < 6) players.push(newPlayer(players.length, 40));
  let decks = $state<DeckSummary[]>([]);

  onMount(async () => {
    try {
      decks = await api<DeckSummary[]>('list_decks');
    } catch {
      decks = [];
    }
  });

  function pickDeck(p: Player, name: string) {
    const d = decks.find((x) => x.name === name);
    p.deck = d?.name ?? null;
    p.image = d?.image ?? '';
    if (d) {
      p.partner = d.commanders.length > 1;
      if (/^Player \d+$/.test(p.name) || decks.some((x) => x.name === p.name || x.commanders[0] === p.name)) {
        p.name = d.commanders[0]?.split(',')[0] || d.name;
      }
    }
  }

  function start() {
    const life = startLife === 0 ? Math.max(1, parseInt(customLife) || 40) : startLife;
    game.start({
      players: players.slice(0, count).map((p, i) => ({ ...p, id: i, name: p.name.trim() || `Player ${i + 1}` })),
      startLife: life,
      tableMode,
      cmdLowersLife,
    });
    enterPlay();
  }

  // --- Play -----------------------------------------------------------------
  let menu = $state(false);
  /** Player whose counters are open full-screen. */
  let countersFor = $state<number | null>(null);
  let roll = $state<{ label: string; value: string } | null>(null);
  let wakeLock: any = null;

  async function enterPlay() {
    game.playing = true;
    try {
      wakeLock = await (navigator as any).wakeLock?.request('screen');
    } catch {
      /* not supported */
    }
  }

  function leavePlay() {
    game.playing = false;
    menu = false;
    try {
      wakeLock?.release();
    } catch {
      /* ignore */
    }
    wakeLock = null;
  }

  onDestroy(leavePlay);

  let s = $derived(game.state);
  let rows = $derived.by(() => {
    if (!s) return [] as Player[][];
    const ps = s.players;
    const top = Math.floor(ps.length / 2);
    return ps.length === 1 ? [ps] : [ps.slice(0, top), ps.slice(top)];
  });

  function dice(sides: number) {
    roll = { label: `d${sides}`, value: String(1 + Math.floor(Math.random() * sides)) };
  }
  function coin() {
    roll = { label: 'Coin', value: Math.random() < 0.5 ? 'Heads' : 'Tails' };
  }
  function first() {
    const id = game.pickFirst();
    const p = s?.players.find((x) => x.id === id);
    roll = { label: 'Goes first', value: p?.name ?? '' };
  }

  function elapsed(ms: number) {
    const m = Math.floor((Date.now() - ms) / 60000);
    return m < 60 ? `${m} min` : `${Math.floor(m / 60)} h ${m % 60} min`;
  }
</script>

{#if game.playing && s}
  <div class="table" class:wide={s.players.length <= 2}>
    {#each rows as row, r (r)}
      <div class="trow" style:--cols={row.length}>
        {#each row as p (p.id)}
          <PlayerPanel player={p} flipped={s.tableMode && rows.length > 1 && r === 0} compact={row.length >= 3} onopen={() => (countersFor = p.id)} />
        {/each}
      </div>
    {/each}

    <button class="hub" onclick={() => (menu = true)} aria-label="Game menu">
      <img src="/icon.svg" alt="" width="30" height="30" />
    </button>
    {#if s.dayNight}<span class="daynight">{s.dayNight === 'day' ? '☀ Day' : '☾ Night'}</span>{/if}
  </div>

  {#if countersFor !== null}
    {@const cp = s.players.find((p) => p.id === countersFor)}
    {#if cp}
      <CountersSheet
        player={cp}
        flipped={s.tableMode && rows.length > 1 && rows[0].some((p) => p.id === cp.id)}
        onclose={() => (countersFor = null)}
      />
    {/if}
  {/if}

  {#if menu}
    <div class="scrim" onclick={() => (menu = false)} role="presentation"></div>
    <div class="menu" role="dialog" aria-modal="true" aria-label="Game menu">
      <div class="menu-head">
        <h2>Game</h2>
        <span class="small muted">{elapsed(s.startedAt)} · {s.players.length} players</span>
        <span class="spacer"></span>
        <button class="btn ghost icon" onclick={() => (menu = false)} aria-label="Close"><Icon name="close" /></button>
      </div>

      <div class="menu-body">
        <div class="grid3">
          <button class="tile" onclick={() => game.undo()} disabled={!game.canUndo}><Icon name="back" /> Undo</button>
          <button class="tile" onclick={() => dice(6)}>Roll d6</button>
          <button class="tile" onclick={() => dice(20)}>Roll d20</button>
          <button class="tile" onclick={coin}>Flip a coin</button>
          <button class="tile" onclick={first}>Pick first player</button>
          <button class="tile" onclick={() => game.setDayNight(s.dayNight === 'day' ? 'night' : s.dayNight === 'night' ? null : 'day')}>
            {s.dayNight === 'day' ? 'It is day' : s.dayNight === 'night' ? 'It is night' : 'Day / night'}
          </button>
        </div>

        {#if roll}
          <div class="roll" aria-live="polite"><span class="small muted">{roll.label}</span><b>{roll.value}</b></div>
        {/if}

        <div class="opts">
          <label><input type="checkbox" checked={s.tableMode} onchange={(e) => game.setOption('tableMode', (e.target as HTMLInputElement).checked)} /> Turn the top row to face players across the table</label>
          <label><input type="checkbox" checked={s.cmdLowersLife} onchange={(e) => game.setOption('cmdLowersLife', (e.target as HTMLInputElement).checked)} /> Commander damage also lowers life</label>
        </div>

        <div class="row end">
          <button class="btn" onclick={() => { game.restart(); menu = false; }}><Icon name="refresh" size={16} /> Restart game</button>
          <button class="btn" onclick={leavePlay}><Icon name="settings" size={16} /> Edit players</button>
          <button class="btn danger" onclick={() => { game.end(); leavePlay(); }}>End game</button>
        </div>
      </div>
    </div>
  {/if}
{:else}
  <header class="head">
    <h1>Game</h1>
    <p class="dim">Track life, commander damage, poison and every other counter for each player. Set up the table, then play on one phone in the middle.</p>
  </header>

  {#if saved && !game.playing && game.state}
    <div class="resume panel">
      <div>
        <b>Game in progress</b>
        <p class="small dim">{game.state.players.map((p) => `${p.name} ${p.life}`).join(' · ')}</p>
      </div>
      <button class="btn primary" onclick={enterPlay}><Icon name="play" size={16} /> Resume</button>
    </div>
  {/if}

  <section class="setup">
    <div class="field-row">
      <span class="lbl">Players</span>
      <div class="segmented" role="group" aria-label="Number of players">
        {#each [2, 3, 4, 5, 6] as n (n)}
          <button aria-pressed={count === n} onclick={() => (count = n)}>{n}</button>
        {/each}
      </div>
    </div>
    <div class="field-row">
      <span class="lbl">Starting life</span>
      <div class="segmented" role="group" aria-label="Starting life">
        {#each [20, 30, 40] as n (n)}
          <button aria-pressed={startLife === n} onclick={() => (startLife = n)}>{n}</button>
        {/each}
        <button aria-pressed={startLife === 0} onclick={() => (startLife = 0)}>Other</button>
      </div>
      {#if startLife === 0}
        <input class="input life-input" type="number" min="1" inputmode="numeric" bind:value={customLife} placeholder="40" aria-label="Custom starting life" />
      {/if}
    </div>

    <ol class="players">
      {#each players.slice(0, count) as p, i (i)}
        <li class="player panel" style:--pc={p.color}>
          <div class="p-top">
            <span class="swatch" aria-hidden="true"></span>
            <input class="input name" bind:value={p.name} aria-label="Player {i + 1} name" />
          </div>
          <div class="colors" role="radiogroup" aria-label="Player {i + 1} color">
            {#each PLAYER_COLORS as c (c)}
              <button role="radio" aria-checked={p.color === c} aria-label="Color" class="color" style:background={c} onclick={() => (p.color = c)}></button>
            {/each}
          </div>
          <div class="p-bottom">
            <select class="input deck" value={p.deck ?? ''} onchange={(e) => pickDeck(p, (e.target as HTMLSelectElement).value)} aria-label="Player {i + 1} deck">
              <option value="">No deck linked</option>
              {#each decks as d (d.name)}<option value={d.name}>{d.name}</option>{/each}
            </select>
            <label class="check"><input type="checkbox" bind:checked={p.partner} /> Partner</label>
          </div>
        </li>
      {/each}
    </ol>

    <div class="opts">
      <label><input type="checkbox" bind:checked={tableMode} /> Turn the top row to face players across the table</label>
      <label><input type="checkbox" bind:checked={cmdLowersLife} /> Commander damage also lowers life</label>
    </div>

    <div>
      <button class="btn primary big" onclick={start}><Icon name="play" size={18} /> {saved ? 'Start a new game' : 'Start game'}</button>
    </div>
    <p class="small muted">Tip: tap a side of a player's panel to lose or gain 1 life, hold to keep counting. Use Counters for commander damage, poison and more.</p>
  </section>
{/if}

<style>
  .head {
    display: grid;
    gap: 0.4rem;
    margin-bottom: 1.5rem;
  }
  .head p {
    max-width: 62ch;
  }
  .resume {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    padding: 0.9rem 1.1rem;
    margin-bottom: 1.5rem;
    border-color: var(--brass);
  }
  .setup {
    display: grid;
    gap: 1.1rem;
    max-width: 52rem;
  }
  .field-row {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    flex-wrap: wrap;
  }
  .lbl {
    min-width: 7rem;
    font-weight: 700;
  }
  .segmented button {
    min-width: 2.75rem;
    min-height: 40px;
  }
  .life-input {
    width: 6rem;
  }
  .players {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 0.75rem;
  }
  .player {
    display: grid;
    gap: 0.55rem;
    padding: 0.75rem;
    border-left: 4px solid var(--pc);
  }
  .p-top,
  .p-bottom {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .swatch {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--pc);
    flex-shrink: 0;
  }
  .name,
  .deck {
    flex: 1;
    min-width: 0;
  }
  .colors {
    display: flex;
    gap: 0.4rem;
  }
  .color {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
  }
  .color[aria-checked='true'] {
    border-color: var(--text);
    box-shadow: 0 0 0 2px var(--ink);
  }
  .check,
  .opts label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
    white-space: nowrap;
  }
  .opts {
    display: grid;
    gap: 0.5rem;
  }
  .opts label {
    white-space: normal;
  }
  .check input,
  .opts input {
    accent-color: var(--brass);
    width: 20px;
    height: 20px;
    flex-shrink: 0;
  }
  .big {
    min-height: 48px;
    padding: 0 1.5rem;
    font-size: 1rem;
  }

  /* --- Full-screen table -------------------------------------------------- */
  .table {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: grid;
    grid-auto-rows: 1fr;
    gap: 6px;
    padding: max(6px, env(safe-area-inset-top)) max(6px, env(safe-area-inset-right)) max(6px, env(safe-area-inset-bottom)) max(6px, env(safe-area-inset-left));
    background: #0d0a12;
    height: 100vh;
    height: 100dvh;
  }
  .trow {
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    gap: 6px;
    min-height: 0;
  }
  /* Landscape: two players side by side instead of stacked. */
  @media (orientation: landscape) {
    .table.wide {
      grid-auto-flow: column;
      grid-auto-columns: 1fr;
      grid-auto-rows: auto;
    }
  }
  .hub {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    z-index: 5;
    display: grid;
    place-items: center;
    width: 54px;
    height: 54px;
    border-radius: 50%;
    border: 3px solid #0d0a12;
    background: #1a1622;
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.55);
    cursor: pointer;
  }
  .daynight {
    position: absolute;
    left: 50%;
    top: calc(50% + 34px);
    transform: translateX(-50%);
    z-index: 5;
    padding: 0.1rem 0.6rem;
    border-radius: 99px;
    background: #1a1622;
    color: #f2d58a;
    font-size: 0.78rem;
    font-weight: 700;
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 75;
    background: rgba(5, 3, 9, 0.6);
  }
  .menu {
    position: fixed;
    z-index: 76;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: min(440px, calc(100vw - 1.5rem));
    max-height: calc(100dvh - 1.5rem);
    overflow: auto;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    box-shadow: var(--shadow);
  }
  .menu-head {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.8rem 0.8rem 0.4rem 1.1rem;
  }
  .menu-body {
    display: grid;
    gap: 1rem;
    padding: 0.4rem 1.1rem 1.1rem;
  }
  .grid3 {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.5rem;
  }
  .tile {
    display: grid;
    place-items: center;
    gap: 0.2rem;
    min-height: 62px;
    padding: 0.4rem;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--panel-2);
    font-weight: 700;
    font-size: 0.85rem;
    cursor: pointer;
    text-align: center;
  }
  .tile:disabled {
    opacity: 0.45;
  }
  .roll {
    display: flex;
    align-items: baseline;
    justify-content: center;
    gap: 0.7rem;
    padding: 0.6rem;
    border-radius: var(--r-sm);
    background: var(--ink-2);
  }
  .roll b {
    font-family: var(--display);
    font-size: 2rem;
    font-weight: 400;
    color: var(--brass-strong);
  }
  .end {
    justify-content: flex-end;
  }

  @media (max-width: 640px) {
    .lbl {
      min-width: 100%;
    }
    .players {
      grid-template-columns: 1fr;
    }
    .segmented {
      flex-wrap: wrap;
    }
  }
</style>
