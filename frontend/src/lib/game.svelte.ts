// Game tracker state: life totals, commander damage, counters, turns and the
// chess clock. The game in progress is kept on this device (localStorage) so
// it survives closing the app; finished games are saved to the engine for
// history and stats.

export interface CounterDef {
  id: string;
  label: string;
  /** Value at which the player loses (e.g. 10 poison). */
  lethal?: number;
}

export const COUNTERS: CounterDef[] = [
  { id: 'poison', label: 'Poison', lethal: 10 },
  { id: 'tax', label: 'Commander tax' },
  { id: 'energy', label: 'Energy' },
  { id: 'experience', label: 'Experience' },
  { id: 'rad', label: 'Rad' },
  { id: 'tickets', label: 'Tickets' },
];

export const PLAYER_COLORS = ['#c9a45c', '#4f93d6', '#d0605a', '#4fa36a', '#9a76c9', '#d48a3c'];
export const MANA_COLORS = ['W', 'U', 'B', 'R', 'G', 'C'] as const;
export const CMD_LETHAL = 21;

/** Magic game modes the tracker supports: starting life, commander damage
 *  (null = no commander damage) and the usual player count. */
export interface GameMode {
  id: string;
  label: string;
  life: number;
  cmd: number | null;
  players: number;
  hint: string;
}

export const GAME_MODES: GameMode[] = [
  { id: 'commander', label: 'Commander', life: 40, cmd: 21, players: 4, hint: '40 life, 21 commander damage.' },
  { id: 'pdh', label: 'Pauper Commander', life: 30, cmd: 16, players: 4, hint: '30 life, 16 commander damage.' },
  { id: 'pdh1v1', label: 'PDH one-on-one', life: 25, cmd: 21, players: 2, hint: 'Pauper Commander duel: 25 life, 21 commander damage.' },
  { id: 'canlander', label: 'Canadian Highlander', life: 20, cmd: null, players: 2, hint: '20 life, one-on-one, no commander.' },
  { id: 'brawl', label: 'Brawl', life: 25, cmd: null, players: 2, hint: '25 life, no commander damage.' },
  { id: 'oathbreaker', label: 'Oathbreaker', life: 20, cmd: null, players: 4, hint: '20 life, no commander damage.' },
  { id: 'constructed', label: '60-card', life: 20, cmd: null, players: 2, hint: 'Standard, Modern, Pauper and other 60-card formats: 20 life.' },
];

/**
 * Look up a game mode by id. Unknown ids (old saves) fall back to the first,
 * four-player Commander.
 */
export function gameMode(id: string | undefined): GameMode {
  return GAME_MODES.find((m) => m.id === id) ?? GAME_MODES[0];
}

/** Commander damage that knocks a player out in this game (null = none). */
export function cmdLethalOf(s: Pick<GameState, 'cmdLethal'> | null | undefined): number | null {
  return s?.cmdLethal === undefined ? CMD_LETHAL : s.cmdLethal;
}
export const LOW_LIFE = 10;

export interface Player {
  id: number;
  name: string;
  color: string;
  life: number;
  partner: boolean;
  image: string;
  deck: string | null;
  counters: Record<string, number>;
  /** Commander damage received, keyed `${fromPlayerId}:${0|1}` (1 = partner). */
  cmd: Record<string, number>;
  custom: { name: string; value: number }[];
  /** Chess clock time left, ms. */
  clockMs?: number;
  /** Player conceded or was knocked out manually. */
  conceded?: boolean;
  /** Panel rotation chosen by this player (degrees); unset = follow the layout. */
  rotation?: Rotation | null;
}

export type Rotation = 0 | 90 | 180 | 270;
/** How panels face: `table` = top row faces across, `sides` = outer columns face
 *  the long edges, `facing` = everything faces the phone's owner. */
export type Layout = 'table' | 'sides' | 'facing';
export const LAYOUTS: { id: Layout; label: string; hint: string }[] = [
  { id: 'table', label: 'Across', hint: 'Top row faces the players across the table' },
  { id: 'sides', label: 'Sides', hint: 'Left and right columns face the long edges, like Mythic Tools' },
  { id: 'facing', label: 'All me', hint: 'Every panel faces you' },
];

export interface LogEntry {
  at: number;
  player: number;
  text: string;
  life: number;
}

export interface GameState {
  players: Player[];
  startLife: number;
  startedAt: number;
  monarch: number | null;
  initiative: number | null;
  blessing: number[];
  dayNight: 'day' | 'night' | null;
  tableMode: boolean;
  /** Game mode id (see GAME_MODES); older saves are Commander. */
  mode?: string;
  /** Commander damage that loses the game; null when the mode has none. */
  cmdLethal?: number | null;
  /** Panel orientation preset; older saves fall back to tableMode. */
  layout?: Layout;
  cmdLowersLife: boolean;
  lowLifeFx: boolean;
  firstPlayer: number | null;
  /** Turn tracking. */
  turns: boolean;
  active: number | null;
  turn: number;
  /** Chess clock: minutes per player, 0 = off. */
  clockMinutes: number;
  clockRunning: boolean;
  log: LogEntry[];
  mana: Record<string, number>;
}

export interface GameRecord {
  date: number;
  mode?: string;
  minutes: number;
  turns: number;
  winner: string | null;
  players: { name: string; deck: string | null; life: number; won: boolean }[];
}

const KEY = 'commanderlab.game.v2';
const MAX_UNDO = 200;
const MAX_LOG = 600;

/**
 * The saved game from localStorage, or null (none, unreadable, or storage
 * blocked).
 */
function load(): GameState | null {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? (JSON.parse(raw) as GameState) : null;
  } catch {
    return null;
  }
}

/**
 * A fresh seat: default name and colour, starting life, no counters or
 * commander damage.
 */
export function newPlayer(id: number, startLife: number): Player {
  return {
    id,
    name: `Player ${id + 1}`,
    color: PLAYER_COLORS[id % PLAYER_COLORS.length],
    life: startLife,
    partner: false,
    image: '',
    deck: null,
    counters: {},
    cmd: {},
    custom: [],
  };
}

/**
 * Why a player has lost, or null if still in: conceded, 0 life, 10 poison,
 * lethal commander damage from any one commander, or (with the chess clock)
 * out of time.
 */
export function isOut(p: Player, clockOn = false, cmdLethal: number | null = CMD_LETHAL): string | null {
  if (p.conceded) return 'Conceded';
  if (p.life <= 0) return 'Out of life';
  if ((p.counters.poison ?? 0) >= 10) return '10 poison';
  if (cmdLethal != null) for (const v of Object.values(p.cmd)) if (v >= cmdLethal) return `${cmdLethal} commander damage`;
  if (clockOn && (p.clockMs ?? 1) <= 0) return 'Out of time';
  return null;
}

/** Seats in clockwise order: top row left→right, then bottom row right→left. */
export function seatOrder(players: Player[]): number[] {
  const top = Math.floor(players.length / 2);
  return [...players.slice(0, top), ...players.slice(top).reverse()].map((p) => p.id);
}

/**
 * The seating layout. Saves from before layouts existed only had
 * `tableMode`.
 */
export function layoutOf(s: Pick<GameState, 'layout' | 'tableMode'>): Layout {
  return s.layout ?? (s.tableMode ? 'table' : 'facing');
}

/** Whether the screen is wider than tall (phone held long ways). */
export const screen = $state({ wide: typeof window !== 'undefined' && window.innerWidth > window.innerHeight });
if (typeof window !== 'undefined') {
  /** Re-measure on resize and rotation. */
  const update = () => (screen.wide = window.innerWidth > window.innerHeight);
  window.addEventListener('resize', update);
  window.addEventListener('orientationchange', update);
}

/** Rotation the layout gives the player at `index` (players fill the top row first).
 *  `sides` faces the screen's long edges: the left and right columns on a tall
 *  screen, the top and bottom rows on a wide one (phone held long ways). */
export function layoutRotation(layout: Layout, index: number, count: number, wide = screen.wide): Rotation {
  if (layout === 'facing' || count < 2) return 0;
  const top = Math.floor(count / 2);
  const inTop = index < top;
  if (layout === 'table' || (layout === 'sides' && wide)) return inTop ? 180 : 0;
  const rowLen = inTop ? top : count - top;
  const col = inTop ? index : index - top;
  if (rowLen >= 2 && col === 0) return 90;
  if (rowLen >= 2 && col === rowLen - 1) return 270;
  return inTop ? 180 : 0;
}

/** The rotation a player's panel is drawn at: their own choice, else the layout's. */
export function panelRotation(s: GameState, id: number): Rotation {
  const i = s.players.findIndex((p) => p.id === id);
  const p = s.players[i];
  if (!p) return 0;
  return p.rotation ?? layoutRotation(layoutOf(s), i, s.players.length);
}

/** Chess clock text, "m:ss", rounded up so 0:00 only shows at zero. */
export function fmtClock(ms: number) {
  const s = Math.max(0, Math.ceil(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

type Setup = Pick<GameState, 'players' | 'startLife' | 'tableMode' | 'layout' | 'mode' | 'cmdLethal' | 'cmdLowersLife' | 'lowLifeFx' | 'turns' | 'clockMinutes'>;

class GameStore {
  state = $state<GameState | null>(load());
  private history: string[] = [];
  canUndo = $state(false);
  /** Brief "+3 / −2" shown on a player's panel after changes. */
  deltas = $state<Record<number, { value: number; at: number }>>({});
  playing = $state(false);
  /** Commander-damage mode: whose commander is dealing damage. */
  cmdFrom = $state<{ player: number; partner: 0 | 1 } | null>(null);
  /** Token being dragged onto a player. */
  dragging = $state<{ kind: 'monarch' | 'initiative'; x: number; y: number } | null>(null);
  private lastTick = 0;

  private save() {
    try {
      if (this.state) localStorage.setItem(KEY, JSON.stringify(this.state));
      else localStorage.removeItem(KEY);
    } catch {
      /* storage unavailable: the game still works for this session */
    }
  }

  private snapshot() {
    if (!this.state) return;
    this.history.push(JSON.stringify(this.state));
    if (this.history.length > MAX_UNDO) this.history.shift();
    this.canUndo = true;
  }

  undo() {
    const prev = this.history.pop();
    if (prev) this.state = JSON.parse(prev);
    this.canUndo = this.history.length > 0;
    this.save();
  }

  player(id: number) {
    return this.state?.players.find((p) => p.id === id);
  }

  private log(id: number, text: string) {
    const s = this.state;
    const p = this.player(id);
    if (!s || !p) return;
    const last = s.log[0];
    // Merge rapid taps on the same player into one entry ("−1, −1" → "−2").
    const m = /^Life ([+−-]\d+)$/.exec(text);
    const lm = last && /^Life ([+−-]\d+)$/.exec(last.text);
    if (m && lm && last.player === id && Date.now() - last.at < 2500) {
      const total = parseInt(lm[1].replace('−', '-')) + parseInt(m[1].replace('−', '-'));
      last.text = `Life ${total >= 0 ? '+' : '−'}${Math.abs(total)}`;
      last.life = p.life;
      last.at = Date.now();
      return;
    }
    s.log.unshift({ at: Date.now(), player: id, text, life: p.life });
    if (s.log.length > MAX_LOG) s.log.length = MAX_LOG;
  }

  start(setup: Setup) {
    const clockMs = setup.clockMinutes > 0 ? setup.clockMinutes * 60000 : undefined;
    this.state = {
      ...setup,
      players: setup.players.map((p) => ({
        ...p,
        life: setup.startLife,
        counters: {},
        cmd: {},
        conceded: false,
        clockMs,
        custom: p.custom.map((c) => ({ ...c, value: 0 })),
      })),
      startedAt: Date.now(),
      monarch: null,
      initiative: null,
      blessing: [],
      dayNight: null,
      firstPlayer: null,
      active: null,
      turn: 0,
      clockRunning: false,
      log: [],
      mana: {},
    };
    this.history = [];
    this.canUndo = false;
    this.deltas = {};
    this.cmdFrom = null;
    this.playing = true;
    this.save();
  }

  restart() {
    if (!this.state) return;
    this.snapshot();
    const s = this.state;
    const clockMs = s.clockMinutes > 0 ? s.clockMinutes * 60000 : undefined;
    for (const p of s.players) {
      p.life = s.startLife;
      p.counters = {};
      p.cmd = {};
      p.conceded = false;
      p.clockMs = clockMs;
      p.custom = p.custom.map((c) => ({ ...c, value: 0 }));
    }
    Object.assign(s, { monarch: null, initiative: null, firstPlayer: null, active: null, turn: 0, clockRunning: false, dayNight: null });
    s.blessing = [];
    s.log = [];
    s.mana = {};
    s.startedAt = Date.now();
    this.deltas = {};
    this.cmdFrom = null;
    this.save();
  }

  /** Summary of the finished game for history and stats. */
  record(winner: number | null): GameRecord | null {
    const s = this.state;
    if (!s) return null;
    return {
      date: Date.now(),
      mode: gameMode(s.mode).label,
      minutes: Math.round((Date.now() - s.startedAt) / 60000),
      turns: s.turn,
      winner: winner == null ? null : (this.player(winner)?.name ?? null),
      players: s.players.map((p) => ({ name: p.name, deck: p.deck, life: p.life, won: p.id === winner })),
    };
  }

  end() {
    this.state = null;
    this.playing = false;
    this.history = [];
    this.canUndo = false;
    this.cmdFrom = null;
    this.save();
  }

  private showDelta(id: number, by: number) {
    const now = Date.now();
    const prev = this.deltas[id];
    const value = prev && now - prev.at < 1800 ? prev.value + by : by;
    this.deltas = { ...this.deltas, [id]: { value, at: now } };
  }

  changeLife(id: number, by: number) {
    const p = this.player(id);
    if (!p || !by) return;
    this.snapshot();
    p.life += by;
    this.showDelta(id, by);
    this.log(id, `Life ${by > 0 ? '+' : '−'}${Math.abs(by)}`);
    this.save();
  }

  setLife(id: number, value: number) {
    const p = this.player(id);
    if (!p || !Number.isFinite(value) || value === p.life) return;
    this.snapshot();
    const by = value - p.life;
    p.life = value;
    this.showDelta(id, by);
    this.log(id, `Life set to ${value}`);
    this.save();
  }

  changeCounter(id: number, counter: string, by: number) {
    const p = this.player(id);
    if (!p) return;
    const next = Math.max(0, (p.counters[counter] ?? 0) + by);
    if (next === (p.counters[counter] ?? 0)) return;
    this.snapshot();
    p.counters[counter] = next;
    const label = COUNTERS.find((c) => c.id === counter)?.label ?? counter;
    this.log(id, `${label} ${by > 0 ? '+' : '−'}${Math.abs(by)} (${next})`);
    this.save();
  }

  changeCmd(id: number, from: number, partner: 0 | 1, by: number) {
    const p = this.player(id);
    const src = this.player(from);
    if (!p || !src || !this.state) return;
    const key = `${from}:${partner}`;
    const current = p.cmd[key] ?? 0;
    const next = Math.max(0, current + by);
    const applied = next - current;
    if (!applied) return;
    this.snapshot();
    p.cmd[key] = next;
    if (this.state.cmdLowersLife) {
      p.life -= applied;
      this.showDelta(id, -applied);
    }
    this.log(id, `Commander damage from ${src.name}${src.partner ? (partner ? ' (partner)' : '') : ''} ${applied > 0 ? '+' : '−'}${Math.abs(applied)} (${next})`);
    this.save();
  }

  addCustom(id: number, name: string) {
    const p = this.player(id);
    const clean = name.trim();
    if (!p || !clean || p.custom.some((c) => c.name.toLowerCase() === clean.toLowerCase())) return;
    this.snapshot();
    p.custom.push({ name: clean, value: 0 });
    this.save();
  }

  changeCustom(id: number, index: number, by: number) {
    const p = this.player(id);
    const c = p?.custom[index];
    if (!c) return;
    const next = Math.max(0, c.value + by);
    if (next === c.value) return;
    this.snapshot();
    c.value = next;
    this.log(id, `${c.name} ${by > 0 ? '+' : '−'}${Math.abs(by)} (${next})`);
    this.save();
  }

  removeCustom(id: number, index: number) {
    const p = this.player(id);
    if (!p) return;
    this.snapshot();
    p.custom.splice(index, 1);
    this.save();
  }

  setDesignation(kind: 'monarch' | 'initiative', id: number | null, toggle = true) {
    if (!this.state) return;
    if (!toggle && this.state[kind] === id) return;
    this.snapshot();
    this.state[kind] = toggle && this.state[kind] === id ? null : id;
    if (this.state[kind] != null) this.log(this.state[kind]!, kind === 'monarch' ? 'Became the Monarch' : 'Took the Initiative');
    this.save();
  }

  toggleBlessing(id: number) {
    if (!this.state) return;
    this.snapshot();
    const b = this.state.blessing;
    this.state.blessing = b.includes(id) ? b.filter((x) => x !== id) : [...b, id];
    this.save();
  }

  setDayNight(v: 'day' | 'night' | null) {
    if (!this.state) return;
    this.snapshot();
    this.state.dayNight = v;
    this.save();
  }

  setImage(id: number, image: string) {
    const p = this.player(id);
    if (!p) return;
    p.image = image;
    this.save();
  }

  concede(id: number) {
    const p = this.player(id);
    if (!p) return;
    this.snapshot();
    p.conceded = !p.conceded;
    this.log(id, p.conceded ? 'Conceded' : 'Back in the game');
    if (p.conceded && this.state?.active === id) this.nextTurn();
    this.save();
  }

  // --- Turns and chess clock -------------------------------------------------

  pickFirst() {
    const s = this.state;
    if (!s) return null;
    const ps = s.players.filter((p) => !isOut(p, s.clockMinutes > 0, cmdLethalOf(s)));
    const chosen = ps[Math.floor(Math.random() * ps.length)]?.id ?? null;
    s.firstPlayer = chosen;
    if (s.turns && chosen != null && s.turn === 0) {
      s.active = chosen;
      s.turn = 1;
      s.clockRunning = s.clockMinutes > 0;
      this.lastTick = Date.now();
    }
    this.save();
    return chosen;
  }

  startTurns(first: number) {
    const s = this.state;
    if (!s) return;
    this.snapshot();
    s.active = first;
    s.turn = Math.max(1, s.turn);
    s.clockRunning = s.clockMinutes > 0;
    this.lastTick = Date.now();
    this.save();
  }

  nextTurn() {
    const s = this.state;
    if (!s) return;
    this.tick();
    const order = seatOrder(s.players).filter((id) => !isOut(this.player(id)!, s.clockMinutes > 0, cmdLethalOf(s)));
    if (!order.length) return;
    this.snapshot();
    if (s.active == null) {
      s.active = s.firstPlayer ?? order[0];
      s.turn = 1;
    } else {
      const i = order.indexOf(s.active);
      const next = order[(i + 1) % order.length];
      // A full round has passed when we come back to the first player.
      if (next === (s.firstPlayer ?? order[0]) || i === -1) s.turn += 1;
      s.active = next;
    }
    if (s.clockMinutes > 0) s.clockRunning = true;
    this.lastTick = Date.now();
    this.save();
  }

  toggleClock() {
    const s = this.state;
    if (!s || s.clockMinutes <= 0) return;
    this.tick();
    s.clockRunning = !s.clockRunning;
    this.lastTick = Date.now();
    this.save();
  }

  /** Called every second by the table while playing. */
  tick() {
    const s = this.state;
    const now = Date.now();
    if (!s || !s.clockRunning || s.active == null || s.clockMinutes <= 0) {
      this.lastTick = now;
      return;
    }
    const p = this.player(s.active);
    if (!p) return;
    const elapsed = this.lastTick ? now - this.lastTick : 0;
    this.lastTick = now;
    p.clockMs = Math.max(0, (p.clockMs ?? s.clockMinutes * 60000) - elapsed);
    if (p.clockMs <= 0) {
      this.log(p.id, 'Ran out of time');
      s.clockRunning = false;
      this.nextTurn();
    }
  }

  // --- Mana pool --------------------------------------------------------------

  changeMana(color: string, by: number) {
    const s = this.state;
    if (!s) return;
    s.mana[color] = Math.max(0, (s.mana[color] ?? 0) + by);
    this.save();
  }

  clearMana() {
    if (!this.state) return;
    this.state.mana = {};
    this.save();
  }

  setLayout(layout: Layout) {
    if (!this.state) return;
    this.state.layout = layout;
    this.state.tableMode = layout !== 'facing';
    // A new layout is a fresh start for everyone's orientation.
    for (const p of this.state.players) p.rotation = null;
    this.save();
  }

  /** Turn one player's panel a quarter turn clockwise (or back to the layout with null). */
  rotatePlayer(id: number, rotation?: Rotation | null) {
    const s = this.state;
    const p = this.player(id);
    if (!s || !p) return;
    p.rotation = rotation === undefined ? (((panelRotation(s, id) + 90) % 360) as Rotation) : rotation;
    this.save();
  }

  /** Set a counter to an exact value (from the quick editor). */
  setCounter(id: number, counter: string, value: number) {
    const p = this.player(id);
    if (!p) return;
    this.changeCounter(id, counter, Math.max(0, value) - (p.counters[counter] ?? 0));
  }

  setCustom(id: number, index: number, value: number) {
    const c = this.player(id)?.custom[index];
    if (c) this.changeCustom(id, index, Math.max(0, value) - c.value);
  }

  setOption<K extends 'tableMode' | 'cmdLowersLife' | 'lowLifeFx' | 'turns'>(key: K, value: boolean) {
    if (!this.state) return;
    this.state[key] = value;
    if (key === 'turns' && !value) {
      this.state.active = null;
      this.state.clockRunning = false;
    }
    this.save();
  }
}

export const game = new GameStore();
