// Game tracker state: life totals, commander damage and counters per player.
// Kept on this device (localStorage) so a game survives closing the app.

export interface CounterDef {
  id: string;
  label: string;
  /** Value at which the player loses (e.g. 10 poison). */
  lethal?: number;
  hint?: string;
}

export const COUNTERS: CounterDef[] = [
  { id: 'poison', label: 'Poison', lethal: 10, hint: '10 poison counters and you lose' },
  { id: 'tax', label: 'Commander tax', hint: 'Times cast from the command zone (+2 mana each)' },
  { id: 'energy', label: 'Energy' },
  { id: 'experience', label: 'Experience' },
  { id: 'rad', label: 'Rad' },
  { id: 'tickets', label: 'Tickets' },
];

export const PLAYER_COLORS = ['#c9a45c', '#4f93d6', '#d0605a', '#4fa36a', '#9a76c9', '#d48a3c'];

export const CMD_LETHAL = 21;

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
  cmdLowersLife: boolean;
  firstPlayer: number | null;
}

const KEY = 'commanderlab.game.v1';
const MAX_UNDO = 150;

function load(): GameState | null {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? (JSON.parse(raw) as GameState) : null;
  } catch {
    return null;
  }
}

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

export function isOut(p: Player): string | null {
  if (p.life <= 0) return 'Out of life';
  if ((p.counters.poison ?? 0) >= 10) return '10 poison';
  for (const v of Object.values(p.cmd)) if (v >= CMD_LETHAL) return '21 commander damage';
  return null;
}

class GameStore {
  state = $state<GameState | null>(load());
  /** Undo snapshots (JSON). */
  private history: string[] = [];
  canUndo = $state(false);
  /** Brief "+3 / −2" shown on a player's panel after changes. */
  deltas = $state<Record<number, { value: number; at: number }>>({});
  playing = $state(false);

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

  start(setup: Omit<GameState, 'startedAt' | 'monarch' | 'initiative' | 'blessing' | 'dayNight' | 'firstPlayer'>) {
    this.state = {
      ...setup,
      players: setup.players.map((p) => ({ ...p, life: setup.startLife, counters: {}, cmd: {}, custom: p.custom.map((c) => ({ ...c, value: 0 })) })),
      startedAt: Date.now(),
      monarch: null,
      initiative: null,
      blessing: [],
      dayNight: null,
      firstPlayer: null,
    };
    this.history = [];
    this.canUndo = false;
    this.deltas = {};
    this.playing = true;
    this.save();
  }

  restart() {
    if (!this.state) return;
    this.snapshot();
    const s = this.state;
    for (const p of s.players) {
      p.life = s.startLife;
      p.counters = {};
      p.cmd = {};
      p.custom = p.custom.map((c) => ({ ...c, value: 0 }));
    }
    s.monarch = s.initiative = s.firstPlayer = null;
    s.blessing = [];
    s.dayNight = null;
    s.startedAt = Date.now();
    this.deltas = {};
    this.save();
  }

  end() {
    this.state = null;
    this.playing = false;
    this.history = [];
    this.canUndo = false;
    this.save();
  }

  private player(id: number) {
    return this.state?.players.find((p) => p.id === id);
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
    this.save();
  }

  changeCounter(id: number, counter: string, by: number) {
    const p = this.player(id);
    if (!p) return;
    const next = Math.max(0, (p.counters[counter] ?? 0) + by);
    if (next === (p.counters[counter] ?? 0)) return;
    this.snapshot();
    p.counters[counter] = next;
    this.save();
  }

  changeCmd(id: number, from: number, partner: 0 | 1, by: number) {
    const p = this.player(id);
    if (!p || !this.state) return;
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
    this.save();
  }

  removeCustom(id: number, index: number) {
    const p = this.player(id);
    if (!p) return;
    this.snapshot();
    p.custom.splice(index, 1);
    this.save();
  }

  setDesignation(kind: 'monarch' | 'initiative', id: number | null) {
    if (!this.state) return;
    this.snapshot();
    this.state[kind] = this.state[kind] === id ? null : id;
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

  pickFirst() {
    if (!this.state) return null;
    const ps = this.state.players;
    const chosen = ps[Math.floor(Math.random() * ps.length)].id;
    this.state.firstPlayer = chosen;
    this.save();
    return chosen;
  }

  setOption<K extends 'tableMode' | 'cmdLowersLife'>(key: K, value: boolean) {
    if (!this.state) return;
    this.state[key] = value;
    this.save();
  }
}

export const game = new GameStore();
