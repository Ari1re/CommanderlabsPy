<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { api } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { cmdLethalOf, fmtClock, game, GAME_MODES, gameMode, isOut, LAYOUTS, layoutOf, MANA_COLORS, newPlayer, panelRotation, PLAYER_COLORS, type GameRecord, type Layout, type Player } from '../lib/game.svelte';
  import type { CardView, DeckSummary } from '../lib/types';
  import { symbolUrl } from '../lib/util';
  import Icon from '../components/Icon.svelte';
  import PlayerPanel from '../components/game/PlayerPanel.svelte';
  import CountersSheet from '../components/game/CountersSheet.svelte';

  // --- Setup ----------------------------------------------------------------
  const saved = game.state;
  let count = $state(saved?.players.length ?? 4);
  let mode = $state(saved?.mode ?? 'commander');
  let startLife = $state(saved?.startLife ?? 40);
  let customLife = $state('');
  let layout = $state<Layout>(saved ? layoutOf(saved) : 'table');
  let cmdLowersLife = $state(saved?.cmdLowersLife ?? true);
  let lowLifeFx = $state(saved?.lowLifeFx ?? true);
  let turns = $state(saved?.turns ?? true);
  let clockMinutes = $state(saved?.clockMinutes ?? 0);
  let players = $state<Player[]>(saved?.players.map((p) => ({ ...p })) ?? Array.from({ length: 6 }, (_, i) => newPlayer(i, 40)));
  while (players.length < 6) players.push(newPlayer(players.length, 40));
  let decks = $state<DeckSummary[]>([]);
  let history = $state<(GameRecord & { id: number })[]>([]);

  onMount(async () => {
    try {
      decks = await api<DeckSummary[]>('list_decks');
    } catch {
      decks = [];
    }
    loadHistory();
  });

  async function loadHistory() {
    try {
      history = await api('games');
    } catch {
      history = [];
    }
  }

  function pickDeck(p: Player, name: string) {
    const d = decks.find((x) => x.name === name);
    p.deck = d?.name ?? null;
    p.image = d?.image ?? '';
    if (d) {
      p.partner = d.commanders.length > 1;
      if (/^Player \d+$/.test(p.name) || decks.some((x) => x.name === p.name || x.commanders[0]?.split(',')[0] === p.name)) {
        p.name = d.commanders[0]?.split(',')[0] || d.name;
      }
    }
  }

  let modeInfo = $derived(gameMode(mode));

  function pickMode(id: string) {
    const m = gameMode(id);
    mode = m.id;
    startLife = m.life;
    count = m.players;
  }

  function start() {
    const life = startLife === 0 ? Math.max(1, parseInt(customLife) || 40) : startLife;
    game.start({
      players: players.slice(0, count).map((p, i) => ({ ...p, id: i, name: p.name.trim() || `Player ${i + 1}` })),
      startLife: life,
      tableMode: layout !== 'facing',
      layout,
      mode,
      cmdLethal: modeInfo.cmd,
      cmdLowersLife,
      lowLifeFx,
      turns,
      clockMinutes,
    });
    enterPlay();
  }

  // Stats from saved games, per player name and per deck.
  let stats = $derived.by(() => {
    const by = new Map<string, { label: string; games: number; wins: number }>();
    for (const g of history) {
      for (const p of g.players) {
        const key = p.deck ? `deck:${p.deck}` : `player:${p.name}`;
        const row = by.get(key) ?? { label: p.deck ?? p.name, games: 0, wins: 0 };
        row.games++;
        if (p.won) row.wins++;
        by.set(key, row);
      }
    }
    return [...by.values()].sort((a, b) => b.games - a.games || b.wins - a.wins).slice(0, 10);
  });

  // --- Play -------------------------------------------------------------------
  let menu = $state(false);
  let countersFor = $state<number | null>(null);
  let editing = $state<number | null>(null);
  let editValue = $state('');
  let roll = $state<{ label: string; value: string } | null>(null);
  let panel = $state<'main' | 'log' | 'lookup'>('main');
  let manaOpen = $state(false);
  let ending = $state(false);
  let winner = $state<number | null>(null);
  let lookup = $state('');
  let lookupResults = $state<CardView[]>([]);
  let lookupTimer: ReturnType<typeof setTimeout> | undefined;
  let wakeLock: any = null;
  let ticker: ReturnType<typeof setInterval> | undefined;

  async function enterPlay() {
    game.playing = true;
    clearInterval(ticker);
    ticker = setInterval(() => game.tick(), 1000);
    try {
      wakeLock = await (navigator as any).wakeLock?.request('screen');
    } catch {
      /* not supported */
    }
  }

  function leavePlay() {
    game.playing = false;
    menu = false;
    countersFor = null;
    game.cmdFrom = null;
    clearInterval(ticker);
    try {
      wakeLock?.release();
    } catch {
      /* ignore */
    }
    wakeLock = null;
  }

  onDestroy(leavePlay);

  /** Esc leaves the table on desktop when nothing else is open. */
  function onkey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !game.playing) return;
    if (menu || ending || editing !== null || countersFor !== null || manaOpen || game.cmdFrom) return;
    leavePlay();
  }

  let s = $derived(game.state);
  let clockOn = $derived((s?.clockMinutes ?? 0) > 0);
  let rows = $derived.by(() => {
    if (!s) return [] as Player[][];
    const ps = s.players;
    const top = Math.floor(ps.length / 2);
    return ps.length === 1 ? [ps] : [ps.slice(0, top), ps.slice(top)];
  });
  const rot = (id: number) => (s ? panelRotation(s, id) : 0);
  let activePlayer = $derived(s?.active != null ? s.players.find((p) => p.id === s!.active) : null);

  function openEdit(id: number) {
    const p = game.player(id);
    if (!p) return;
    editing = id;
    editValue = String(p.life);
  }
  function nudgeEdit(by: number) {
    editValue = String((parseInt(editValue) || 0) + by);
  }
  function saveEdit() {
    if (editing != null) game.setLife(editing, parseInt(editValue));
    editing = null;
  }

  function dice(sides: number) {
    roll = { label: `d${sides}`, value: String(1 + Math.floor(Math.random() * sides)) };
  }
  function coin() {
    roll = { label: 'Coin', value: Math.random() < 0.5 ? 'Heads' : 'Tails' };
  }
  function first() {
    const id = game.pickFirst();
    roll = { label: 'Goes first', value: s?.players.find((x) => x.id === id)?.name ?? '' };
  }

  function searchLookup() {
    clearTimeout(lookupTimer);
    const q = lookup.trim();
    if (q.length < 2) {
      lookupResults = [];
      return;
    }
    lookupTimer = setTimeout(async () => {
      try {
        lookupResults = await api<CardView[]>('search_cards', { query: q, limit: 10 });
      } catch {
        lookupResults = [];
      }
    }, 250);
  }

  function beginEnd() {
    const alive = s?.players.filter((p) => !isOut(p, clockOn, cmdLethalOf(s))) ?? [];
    winner = alive.length === 1 ? alive[0].id : null;
    ending = true;
  }

  async function finish(saveIt: boolean) {
    const record = game.record(winner);
    if (saveIt && record) {
      try {
        await api('game_record', { game: record });
        app.toast(record.winner ? `Saved: ${record.winner} won.` : 'Game saved.', 'success');
      } catch (err) {
        app.error(err);
      }
    }
    ending = false;
    game.end();
    leavePlay();
    loadHistory();
  }

  function elapsed(ms: number) {
    const m = Math.floor((Date.now() - ms) / 60000);
    return m < 60 ? `${m} min` : `${Math.floor(m / 60)} h ${m % 60} min`;
  }

  function when(ms: number) {
    return new Date(ms).toLocaleDateString([], { month: 'short', day: 'numeric' });
  }

  function playerName(id: number) {
    return s?.players.find((p) => p.id === id)?.name ?? '';
  }
</script>

<svelte:window onkeydown={onkey} />

{#if game.playing && s}
  <div class="table" class:wide={s.players.length <= 2}>
    {#each rows as row, r (r)}
      <div class="trow" style:--cols={row.length}>
        {#each row as p (p.id)}
          <div class="cell">
            <PlayerPanel
              player={p}
              rotation={rot(p.id)}
              compact={row.length >= 3}
              onopen={() => (countersFor = p.id)}
            />
          </div>
        {/each}
      </div>
    {/each}

    <button class="leave" onclick={leavePlay} aria-label="Leave the table (the game is kept)" title="Leave the table (Esc). The game is kept.">
      <Icon name="back" size={16} /> Leave table
    </button>

    <button class="hub" onclick={() => { game.cmdFrom = null; panel = 'main'; menu = true; }} aria-label="Game menu">
      <img src="/icon.svg" alt="" width="30" height="30" />
    </button>

    {#if game.cmdFrom}
      <button class="pill under" onclick={() => (game.cmdFrom = null)}><Icon name="check" size={16} /> Done with combat</button>
    {:else if s.turns}
      <div class="pill under turn">
        {#if activePlayer}
          <span class="small">Turn {s.turn} · {activePlayer.name}</span>
          {#if clockOn}<button class="mini" onclick={() => game.toggleClock()} aria-label={s.clockRunning ? 'Pause clock' : 'Resume clock'}>{s.clockRunning ? '❚❚' : '▶'}</button>{/if}
          <button class="next" onclick={() => game.nextTurn()}>Next turn</button>
        {:else}
          <button class="next" onclick={first}>Pick who goes first</button>
        {/if}
      </div>
    {/if}
    {#if s.dayNight}<span class="pill over">{s.dayNight === 'day' ? '☀ Day' : '☾ Night'}</span>{/if}

    {#if manaOpen}
      <div class="mana" role="group" aria-label="Mana pool">
        {#each MANA_COLORS as c (c)}
          <button
            class="mana-btn"
            onclick={() => game.changeMana(c, 1)}
            oncontextmenu={(e) => { e.preventDefault(); game.changeMana(c, -1); }}
            aria-label="{c} mana: {s.mana[c] ?? 0}. Tap to add, long-press to remove"
          >
            <img src={symbolUrl(c)} alt="" width="22" height="22" />
            <span class="num">{s.mana[c] ?? 0}</span>
          </button>
        {/each}
        <button class="mana-btn clear" onclick={() => game.clearMana()} aria-label="Empty mana pool">↺</button>
        <button class="mana-btn clear" onclick={() => (manaOpen = false)} aria-label="Hide mana pool"><Icon name="close" size={14} /></button>
      </div>
    {/if}

    {#if game.dragging}
      <span class="drag-ghost" style:left="{game.dragging.x}px" style:top="{game.dragging.y}px">{game.dragging.kind === 'monarch' ? '♛' : '⚔'}</span>
    {/if}
  </div>

  {#if countersFor !== null}
    {@const cp = s.players.find((p) => p.id === countersFor)}
    {#if cp}<CountersSheet player={cp} rotation={rot(cp.id)} onclose={() => (countersFor = null)} onedit={() => openEdit(countersFor ?? -1)} />{/if}
  {/if}

  {#if editing !== null}
    {@const ep = s.players.find((p) => p.id === editing)}
    <div class="scrim" onclick={() => (editing = null)} role="presentation"></div>
    <form class="dialog edit rot{ep ? rot(ep.id) : 0}" onsubmit={(e) => { e.preventDefault(); saveEdit(); }} aria-label="Set life">
      <h2>{ep?.name}'s life</h2>
      <input class="input life-entry num" type="number" inputmode="numeric" bind:value={editValue} aria-label="Life total" />
      <div class="nudges">
        {#each [-100, -10, -5, -1, 1, 5, 10, 100] as n (n)}
          <button type="button" class="nudge" onclick={() => nudgeEdit(n)}>{n > 0 ? `+${n}` : n}</button>
        {/each}
      </div>
      <div class="row end">
        <button type="button" class="btn ghost" onclick={() => (editing = null)}>Cancel</button>
        <button class="btn primary">Set life</button>
      </div>
    </form>
  {/if}

  {#if menu}
    <div class="scrim" onclick={() => (menu = false)} role="presentation"></div>
    <div class="dialog menu" role="dialog" aria-modal="true" aria-label="Game menu">
      <div class="menu-head">
        {#if panel !== 'main'}<button class="btn ghost icon" onclick={() => (panel = 'main')} aria-label="Back"><Icon name="back" /></button>{/if}
        <h2>{panel === 'log' ? 'Life log' : panel === 'lookup' ? 'Look up a card' : 'Game'}</h2>
        {#if panel === 'main'}<span class="small muted">{gameMode(s.mode).label} · {elapsed(s.startedAt)}{s.turns && s.turn ? ` · turn ${s.turn}` : ''}</span>{/if}
        <span class="spacer"></span>
        <button class="btn ghost icon" onclick={() => (menu = false)} aria-label="Close"><Icon name="close" /></button>
      </div>

      {#if panel === 'main'}
        <div class="menu-body">
          <div class="grid3">
            <button class="tile" onclick={() => game.undo()} disabled={!game.canUndo}>Undo</button>
            <button class="tile" onclick={() => dice(6)}>Roll d6</button>
            <button class="tile" onclick={() => dice(20)}>Roll d20</button>
            <button class="tile" onclick={coin}>Flip a coin</button>
            <button class="tile" onclick={first}>Pick first player</button>
            <button class="tile" onclick={() => game.setDayNight(s.dayNight === 'day' ? 'night' : s.dayNight === 'night' ? null : 'day')}>
              {s.dayNight === 'day' ? 'It is day' : s.dayNight === 'night' ? 'It is night' : 'Day / night'}
            </button>
            <button class="tile" onclick={() => { manaOpen = !manaOpen; menu = false; }}>{manaOpen ? 'Hide mana pool' : 'Mana pool'}</button>
            <button class="tile" onclick={() => (panel = 'log')}>Life log</button>
            <button class="tile" onclick={() => (panel = 'lookup')}>Look up a card</button>
          </div>

          {#if roll}
            <div class="roll" aria-live="polite"><span class="small muted">{roll.label}</span><b>{roll.value}</b></div>
          {/if}

          <div class="tokens-row">
            <span class="small muted">Tokens</span>
            <div class="row">
              {#each s.players as p (p.id)}
                <button class="chip-btn" class:on={s.monarch === p.id} onclick={() => game.setDesignation('monarch', p.id)} aria-label="Give Monarch to {p.name}">♛ {p.name}</button>
              {/each}
            </div>
            <div class="row">
              {#each s.players as p (p.id)}
                <button class="chip-btn" class:on={s.initiative === p.id} onclick={() => game.setDesignation('initiative', p.id)} aria-label="Give Initiative to {p.name}">⚔ {p.name}</button>
              {/each}
            </div>
          </div>

          <div class="layout-pick">
            <span class="lbl">Panels face</span>
            <div class="segmented" role="group" aria-label="Life counter layout">
              {#each LAYOUTS as l (l.id)}
                <button aria-pressed={layoutOf(s) === l.id} title={l.hint} onclick={() => game.setLayout(l.id)}>{l.label}</button>
              {/each}
            </div>
            <p class="small dim">{LAYOUTS.find((l) => l.id === layoutOf(s!))?.hint}. Anyone can turn their own panel from their menu.</p>
          </div>

          <div class="opts">
            <label><input type="checkbox" checked={s.turns} onchange={(e) => game.setOption('turns', (e.target as HTMLInputElement).checked)} /> Track turns{clockOn ? ' and chess clock' : ''}</label>
            {#if cmdLethalOf(s) != null}
            <label><input type="checkbox" checked={s.cmdLowersLife} onchange={(e) => game.setOption('cmdLowersLife', (e.target as HTMLInputElement).checked)} /> Commander damage also lowers life</label>
            {/if}
            <label><input type="checkbox" checked={s.lowLifeFx} onchange={(e) => game.setOption('lowLifeFx', (e.target as HTMLInputElement).checked)} /> Pulse when a player is at 10 life or less</label>
          </div>

          <p class="small muted">Leave table goes back to the app (and the setup screen, to edit players). The game is kept; tap Resume to come back.</p>
          <div class="row end">
            <button class="btn" onclick={() => { game.restart(); menu = false; }}><Icon name="refresh" size={16} /> Restart</button>
            <button class="btn" onclick={leavePlay}><Icon name="back" size={16} /> Leave table</button>
            <button class="btn primary" onclick={() => { menu = false; beginEnd(); }}>End game</button>
          </div>
        </div>
      {:else if panel === 'log'}
        <ol class="log">
          {#if !s.log.length}<li class="muted small">Nothing yet. Every life and counter change shows up here.</li>{/if}
          {#each s.log as e, i (i)}
            {@const p = s.players.find((x) => x.id === e.player)}
            <li>
              <span class="dot" style:background={p?.color}></span>
              <span class="who">{p?.name}</span>
              <span class="what">{e.text}</span>
              <span class="num muted small">{e.life}</span>
            </li>
          {/each}
        </ol>
      {:else}
        <div class="menu-body">
          <input class="input" bind:value={lookup} oninput={searchLookup} placeholder="Card name" aria-label="Card name" />
          <ul class="results">
            {#each lookupResults as c (c.oracle_id)}
              <li><button onclick={() => app.open(c)}>{c.name} <span class="small muted">{c.type_line}</span></button></li>
            {/each}
          </ul>
        </div>
      {/if}
    </div>
  {/if}

  {#if ending}
    <div class="scrim" role="presentation"></div>
    <div class="dialog menu" role="dialog" aria-modal="true" aria-label="Who won?">
      <div class="menu-head"><h2>Who won?</h2></div>
      <div class="menu-body">
        <div class="winners">
          {#each s.players as p (p.id)}
            <button class="winner" class:on={winner === p.id} style:--pc={p.color} onclick={() => (winner = p.id)}>
              <span class="dot"></span>{p.name}<span class="num small muted">{p.life}</span>
            </button>
          {/each}
          <button class="winner" class:on={winner === null} onclick={() => (winner = null)}>No winner / draw</button>
        </div>
        <p class="small muted">Saved games build your win rates by player and deck below the setup.</p>
        <div class="row end">
          <button class="btn ghost" onclick={() => (ending = false)}>Keep playing</button>
          <button class="btn" onclick={() => finish(false)}>End without saving</button>
          <button class="btn primary" onclick={() => finish(true)}>Save game</button>
        </div>
      </div>
    </div>
  {/if}
{:else}
  <header class="head">
    <h1>Game</h1>
    <p class="dim">A life counter for the whole table: life, commander damage, poison and every other counter, turns and a chess clock. Set it up, then put your phone in the middle.</p>
  </header>

  {#if game.state && !game.playing}
    <div class="resume panel">
      <div>
        <b>{gameMode(game.state.mode).label} game in progress</b>
        <p class="small dim">{game.state.players.map((p) => `${p.name} ${p.life}`).join(' · ')}</p>
      </div>
      <button class="btn primary" onclick={enterPlay}><Icon name="play" size={16} /> Resume</button>
    </div>
  {/if}

  <section class="setup">
    <div class="field-row">
      <span class="lbl">Game mode</span>
      <div class="segmented modes" role="group" aria-label="Game mode">
        {#each GAME_MODES as m (m.id)}
          <button aria-pressed={mode === m.id} onclick={() => pickMode(m.id)}>{m.label}</button>
        {/each}
      </div>
      <span class="small dim mode-hint">{modeInfo.hint}</span>
    </div>
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
    <div class="field-row">
      <span class="lbl">Chess clock</span>
      <div class="segmented" role="group" aria-label="Chess clock minutes per player">
        {#each [0, 10, 20, 30, 45] as n (n)}
          <button aria-pressed={clockMinutes === n} onclick={() => { clockMinutes = n; if (n) turns = true; }}>{n ? `${n} min` : 'Off'}</button>
        {/each}
      </div>
    </div>

    <div class="field-row">
      <span class="lbl">Panels face</span>
      <div class="segmented" role="group" aria-label="Life counter layout">
        {#each LAYOUTS as l (l.id)}
          <button aria-pressed={layout === l.id} onclick={() => (layout = l.id)}>{l.label}</button>
        {/each}
      </div>
      <span class="small dim layout-hint">{LAYOUTS.find((l) => l.id === layout)?.hint}</span>
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
      <label><input type="checkbox" bind:checked={turns} /> Track turns (whose turn it is, turn count)</label>
      {#if modeInfo.cmd != null}
      <label><input type="checkbox" bind:checked={cmdLowersLife} /> Commander damage also lowers life</label>
      {/if}
      <label><input type="checkbox" bind:checked={lowLifeFx} /> Pulse when a player is at 10 life or less</label>
    </div>

    <div>
      <button class="btn primary big" onclick={start}><Icon name="play" size={18} /> {game.state ? 'Start a new game' : 'Start game'}</button>
    </div>
    <ul class="tips small muted">
      <li>Tap a side of a panel for ±1. Hold for ±10.</li>
      <li>Set an exact life total from the player's menu.</li>
      <li>Swipe up or down on a panel (or tap ⋯) for that player's menu: commander damage, counters, tokens, background art.</li>
      <li>“Deal commander damage” turns every other panel into a damage counter for that commander.</li>
    </ul>
  </section>

  {#if history.length}
    <section class="history">
      <h2>History</h2>
      {#if stats.length}
        <table class="stats">
          <thead><tr><th>Player or deck</th><th class="num">Games</th><th class="num">Wins</th><th class="num">Win rate</th></tr></thead>
          <tbody>
            {#each stats as r (r.label)}
              <tr><td>{r.label}</td><td class="num">{r.games}</td><td class="num">{r.wins}</td><td class="num">{Math.round((r.wins / r.games) * 100)}%</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}
      <ul class="games">
        {#each history.slice(0, 10) as g (g.id)}
          <li>
            <span class="small muted">{when(g.date)}</span>
            <span>{g.winner ? `${g.winner} won` : 'No winner'}</span>
            <span class="small dim">{g.mode ? `${g.mode} · ` : ''}{g.players.map((p) => p.name).join(', ')} · {g.minutes} min{g.turns ? ` · ${g.turns} turns` : ''}</span>
            <button class="btn ghost small icon" aria-label="Delete this game" onclick={async () => (history = await api('game_delete', { id: g.id }))}><Icon name="trash" size={14} /></button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
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
    width: 32px;
    height: 32px;
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
    min-height: 36px;
  }
  .opts {
    display: grid;
    gap: 0.25rem;
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
  .tips {
    margin: 0;
    padding-left: 1.1rem;
    display: grid;
    gap: 0.2rem;
  }
  .history {
    display: grid;
    gap: 0.9rem;
    max-width: 52rem;
    margin-top: 2.5rem;
  }
  .stats {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.92rem;
  }
  .stats th {
    text-align: left;
    font-weight: 700;
    color: var(--text-2);
    padding: 0.4rem 0.5rem;
    border-bottom: 1px solid var(--line);
  }
  .stats td {
    padding: 0.45rem 0.5rem;
    border-bottom: 1px solid var(--line-soft);
  }
  .stats .num {
    text-align: right;
  }
  .games {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
  }
  .games li {
    display: grid;
    grid-template-columns: 4rem 1fr auto;
    grid-template-areas: 'd w x' 'd p x';
    column-gap: 0.75rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--line-soft);
    align-items: center;
  }
  .games li > :nth-child(1) {
    grid-area: d;
  }
  .games li > :nth-child(2) {
    grid-area: w;
    font-weight: 700;
  }
  .games li > :nth-child(3) {
    grid-area: p;
  }
  .games li > :nth-child(4) {
    grid-area: x;
  }

  /* --- Full-screen table -------------------------------------------------- */
  .table {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: grid;
    grid-auto-rows: 1fr;
    gap: 6px;
    padding: max(6px, var(--safe-top)) max(6px, env(safe-area-inset-right)) max(6px, env(safe-area-inset-bottom)) max(6px, env(safe-area-inset-left));
    background: #0d0a12;
    height: 100vh;
    height: 100dvh;
    overscroll-behavior: none;
  }
  .trow {
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    gap: 6px;
    min-height: 0;
  }
  /* Each panel sits in a cell it can measure, so sideways panels can swap
     their width and height. */
  .cell {
    position: relative;
    container-type: size;
    min-width: 0;
    min-height: 0;
  }
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
    width: 56px;
    height: 56px;
    border-radius: 50%;
    border: 3px solid #0d0a12;
    background: #1a1622;
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.55);
    cursor: pointer;
  }
  /* Desktop: a visible way back to the app (on phones it's in the game menu). */
  .leave {
    display: none;
    position: absolute;
    top: max(10px, var(--safe-top));
    left: 50%;
    transform: translateX(-50%);
    z-index: 6;
    align-items: center;
    gap: 0.35rem;
    min-height: 36px;
    padding: 0 0.9rem;
    border-radius: 99px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    background: rgba(13, 10, 18, 0.85);
    color: #fff;
    font-weight: 700;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .leave:hover {
    background: #2a2236;
  }
  @media (hover: hover) and (pointer: fine), (min-width: 1000px) {
    .leave {
      display: inline-flex;
    }
  }
  .pill {
    position: absolute;
    left: 50%;
    z-index: 5;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    transform: translateX(-50%);
    padding: 0.2rem 0.3rem 0.2rem 0.75rem;
    border-radius: 99px;
    background: #1a1622;
    border: 2px solid #0d0a12;
    color: #fff;
    white-space: nowrap;
    font-weight: 700;
  }
  .pill.under {
    top: calc(50% + 34px);
  }
  .pill.over {
    top: calc(50% - 64px);
    padding: 0.1rem 0.7rem;
    color: #f2d58a;
    font-size: 0.8rem;
  }
  button.pill {
    min-height: 40px;
    padding: 0 1rem;
    background: #f2d58a;
    color: #14101b;
    cursor: pointer;
  }
  .next,
  .mini {
    min-height: 34px;
    padding: 0 0.85rem;
    border: 0;
    border-radius: 99px;
    background: #f2d58a;
    color: #14101b;
    font-weight: 700;
    cursor: pointer;
  }
  .mini {
    padding: 0 0.6rem;
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
  }
  .mana {
    position: absolute;
    left: 50%;
    top: calc(50% - 96px);
    z-index: 6;
    transform: translateX(-50%);
    display: flex;
    gap: 4px;
    padding: 5px;
    border-radius: 99px;
    background: #1a1622;
    border: 2px solid #0d0a12;
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.5);
    max-width: calc(100vw - 16px);
  }
  .mana-btn {
    display: grid;
    place-items: center;
    gap: 1px;
    min-width: 38px;
    padding: 3px 4px;
    border: 0;
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.06);
    color: #fff;
    font-weight: 700;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .mana-btn.clear {
    min-width: 32px;
  }
  .drag-ghost {
    position: fixed;
    z-index: 90;
    transform: translate(-50%, -60%);
    display: grid;
    place-items: center;
    width: 50px;
    height: 50px;
    border-radius: 50%;
    border: 2px solid #f2d58a;
    background: rgba(20, 14, 6, 0.9);
    color: #f2d58a;
    font-size: 1.5rem;
    pointer-events: none;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.6);
  }

  .scrim {
    position: fixed;
    inset: 0;
    z-index: 75;
    background: rgba(5, 3, 9, 0.6);
  }
  .dialog {
    position: fixed;
    z-index: 76;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: min(460px, calc(100vw - 1.5rem));
    max-height: calc(100dvh - 1.5rem);
    overflow: auto;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    box-shadow: var(--shadow);
  }
  .dialog.rot180 {
    transform: translate(-50%, -50%) rotate(180deg);
  }
  .dialog.rot90,
  .dialog.rot270 {
    width: min(460px, calc(100dvh - 1.5rem));
    max-height: calc(100dvw - 1.5rem);
  }
  .dialog.rot90 {
    transform: translate(-50%, -50%) rotate(90deg);
  }
  .dialog.rot270 {
    transform: translate(-50%, -50%) rotate(270deg);
  }
  .layout-pick {
    display: grid;
    gap: 0.35rem;
    margin-bottom: 0.6rem;
  }
  .layout-pick p,
  .layout-hint {
    margin: 0;
  }
  .edit {
    display: grid;
    gap: 0.9rem;
    padding: 1.1rem;
  }
  .life-entry {
    font-family: var(--display);
    font-size: 2.6rem;
    text-align: center;
    min-height: 64px;
  }
  .nudges {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.4rem;
  }
  .nudge {
    min-height: 44px;
    border-radius: var(--r-sm);
    border: 1px solid var(--line);
    background: var(--panel-2);
    font-weight: 700;
    cursor: pointer;
  }
  .menu-head {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.8rem 0.8rem 0.4rem 1.1rem;
    background: var(--panel);
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
    min-height: 56px;
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
  .tokens-row {
    display: grid;
    gap: 0.35rem;
  }
  .chip-btn {
    min-height: 34px;
    padding: 0 0.7rem;
    border-radius: 99px;
    border: 1px solid var(--line);
    background: transparent;
    font-size: 0.82rem;
    font-weight: 700;
    cursor: pointer;
  }
  .chip-btn.on {
    border-color: var(--brass);
    color: var(--brass-strong);
    background: color-mix(in srgb, var(--brass) 12%, transparent);
  }
  .end {
    justify-content: flex-end;
  }
  .log {
    list-style: none;
    margin: 0;
    padding: 0.2rem 1.1rem 1.1rem;
    display: grid;
    max-height: 65dvh;
    overflow: auto;
  }
  .log li {
    display: grid;
    grid-template-columns: 10px auto 1fr auto;
    gap: 0.5rem;
    align-items: center;
    padding: 0.45rem 0;
    border-bottom: 1px solid var(--line-soft);
    font-size: 0.9rem;
  }
  .log .who {
    font-weight: 700;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--pc, var(--muted));
  }
  .results {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .results button {
    display: grid;
    width: 100%;
    text-align: left;
    padding: 0.55rem 0.3rem;
    border: 0;
    border-bottom: 1px solid var(--line-soft);
    background: none;
    cursor: pointer;
  }
  .winners {
    display: grid;
    gap: 0.4rem;
  }
  .winner {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    min-height: 46px;
    padding: 0 0.9rem;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--panel-2);
    font-weight: 700;
    text-align: left;
    cursor: pointer;
  }
  .winner .num {
    margin-left: auto;
  }
  .winner.on {
    border-color: var(--brass);
    background: color-mix(in srgb, var(--brass) 14%, var(--panel));
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
    .games li {
      grid-template-columns: 3.5rem 1fr auto;
    }
  }
  .segmented.modes {
    flex-wrap: wrap;
  }
  .mode-hint {
    flex-basis: 100%;
  }
</style>
