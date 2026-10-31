<script lang="ts">
  import { onDestroy } from 'svelte';
  import { COUNTERS, CMD_LETHAL, LOW_LIFE, fmtClock, game, isOut, type Player, type Rotation } from '../../lib/game.svelte';
  import { artCrop } from '../../lib/util';
  import Icon from '../Icon.svelte';

  let {
    player,
    rotation = 0,
    compact = false,
    onopen,
  }: { player: Player; rotation?: Rotation; compact?: boolean; onopen: () => void } = $props();

  let sideways = $derived(rotation === 90 || rotation === 270);

  let now = $state(Date.now());
  const clock = setInterval(() => (now = Date.now()), 400);
  onDestroy(() => {
    clearInterval(clock);
    clearPress();
  });

  let s = $derived(game.state!);
  let clockOn = $derived(s.clockMinutes > 0);
  let out = $derived(isOut(player, clockOn));
  let delta = $derived(game.deltas[player.id]);
  let showDelta = $derived(delta && now - delta.at < 1800 ? delta.value : 0);
  let maxCmd = $derived(Math.max(0, ...Object.values(player.cmd)));
  let active = $derived(s.turns && s.active === player.id);
  let lowLife = $derived(s.lowLifeFx && !out && player.life > 0 && player.life <= LOW_LIFE);
  type Edit = { kind: 'counter'; id: string } | { kind: 'custom'; index: number } | { kind: 'cmd' };
  let badges = $derived([
    ...COUNTERS.filter((c) => player.counters[c.id]).map((c) => ({
      key: `c:${c.id}`,
      label: c.label,
      value: c.id === 'tax' ? `+${player.counters[c.id] * 2}` : String(player.counters[c.id]),
      danger: c.lethal != null && player.counters[c.id] >= c.lethal - 2,
      edit: { kind: 'counter', id: c.id } as Edit,
    })),
    ...player.custom
      .map((c, index) => ({ c, index }))
      .filter(({ c }) => c.value)
      .map(({ c, index }) => ({ key: `x:${c.name}`, label: c.name, value: String(c.value), danger: false, edit: { kind: 'custom', index } as Edit })),
  ]);

  // --- Quick editor for a counter chip, and the token picker. Both live inside
  // the panel so they face the same way as the player.
  let editing = $state<Edit | null>(null);
  let picking = $state<'monarch' | 'initiative' | null>(null);
  let opponents = $derived(s.players.filter((p) => p.id !== player.id));
  let quick = $derived.by(() => {
    const e = editing;
    if (!e || e.kind === 'cmd') return null;
    if (e.kind === 'counter') {
      const def = COUNTERS.find((c) => c.id === e.id);
      const v = player.counters[e.id] ?? 0;
      return { label: def?.label ?? e.id, value: v, note: e.id === 'tax' ? `+${v * 2} mana to cast` : def?.lethal ? `${def.lethal} loses the game` : '' };
    }
    const c = player.custom[e.index];
    return c ? { label: c.name, value: c.value, note: '' } : null;
  });

  function bump(by: number) {
    const e = editing;
    if (!e || e.kind === 'cmd') return;
    if (e.kind === 'counter') game.changeCounter(player.id, e.id, by);
    else game.changeCustom(player.id, e.index, by);
  }
  function clearQuick() {
    const e = editing;
    if (!e || e.kind === 'cmd') return;
    if (e.kind === 'counter') game.setCounter(player.id, e.id, 0);
    else game.setCustom(player.id, e.index, 0);
    editing = null;
  }
  function closeOverlays() {
    editing = null;
    picking = null;
  }

  // Commander-damage mode.
  let cmd = $derived(game.cmdFrom);
  let attacker = $derived(cmd ? game.player(cmd.player) : null);
  let isAttacker = $derived(cmd?.player === player.id);
  let cmdTaken = $derived(cmd && !isAttacker ? (player.cmd[`${cmd.player}:${cmd.partner}`] ?? 0) : 0);

  // --- Gestures: tap ±1, hold ±10 (repeating), vertical swipe opens the menu.
  const HOLD_MS = 450;
  let press: { by: number; x: number; y: number; held: boolean; timer?: ReturnType<typeof setTimeout>; rep?: ReturnType<typeof setInterval> } | null = null;

  function apply(by: number) {
    if (cmd && !isAttacker) game.changeCmd(player.id, cmd.player, cmd.partner, by);
    else game.changeLife(player.id, by);
  }

  function down(by: number, e: PointerEvent) {
    if (cmd && isAttacker) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    const p: NonNullable<typeof press> = { by, x: e.clientX, y: e.clientY, held: false };
    p.timer = setTimeout(() => {
      p.held = true;
      apply(by * 10);
      navigator.vibrate?.(18);
      p.rep = setInterval(() => apply(by * 10), 550);
    }, HOLD_MS);
    press = p;
  }

  function move(e: PointerEvent) {
    if (!press || press.held) return;
    // Measure the swipe in the panel's own direction, so "up" means up for the
    // player even when the panel is turned sideways.
    const sx = e.clientX - press.x;
    const sy = e.clientY - press.y;
    const a = (-rotation * Math.PI) / 180;
    const dx = sx * Math.cos(a) - sy * Math.sin(a);
    const dy = sx * Math.sin(a) + sy * Math.cos(a);
    if (Math.abs(dy) > 45 && Math.abs(dy) > Math.abs(dx) * 1.3) {
      clearPress();
      if (!cmd) onopen();
    } else if (Math.hypot(dx, dy) > 18) {
      clearTimeout(press.timer);
    }
  }

  function up() {
    if (!press) return;
    const p = press;
    clearPress();
    if (!p.held) {
      apply(p.by);
      navigator.vibrate?.(6);
    }
  }

  function clearPress() {
    if (press) {
      clearTimeout(press.timer);
      clearInterval(press.rep);
    }
    press = null;
  }

  // --- Monarch / Initiative token: tap to choose who gets it, or drag it onto
  // another player.
  const TAP_SLOP = 10;
  function dragToken(kind: 'monarch' | 'initiative', e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    const x0 = e.clientX;
    const y0 = e.clientY;
    let dragged = false;
    const moveFn = (ev: PointerEvent) => {
      if (!dragged && Math.hypot(ev.clientX - x0, ev.clientY - y0) < TAP_SLOP) return;
      dragged = true;
      game.dragging = { kind, x: ev.clientX, y: ev.clientY };
    };
    const upFn = (ev: PointerEvent) => {
      window.removeEventListener('pointermove', moveFn);
      window.removeEventListener('pointerup', upFn);
      window.removeEventListener('pointercancel', cancelFn);
      game.dragging = null;
      if (!dragged) {
        editing = null;
        picking = kind;
        return;
      }
      const target = document.elementsFromPoint(ev.clientX, ev.clientY).find((el) => (el as HTMLElement).dataset?.player);
      const id = target ? Number((target as HTMLElement).dataset.player) : null;
      if (id != null && id !== player.id) game.setDesignation(kind, id, false);
    };
    const cancelFn = () => {
      window.removeEventListener('pointermove', moveFn);
      window.removeEventListener('pointerup', upFn);
      window.removeEventListener('pointercancel', cancelFn);
      game.dragging = null;
    };
    window.addEventListener('pointermove', moveFn);
    window.addEventListener('pointerup', upFn);
    window.addEventListener('pointercancel', cancelFn);
  }

  function give(id: number | null) {
    if (!picking) return;
    if (id == null) {
      if (s[picking] != null) game.setDesignation(picking, s[picking], true);
    } else {
      game.setDesignation(picking, id, false);
    }
    picking = null;
  }
</script>

<section
  class="panel rot{rotation}"
  class:sideways
  class:out={!!out}
  class:compact
  class:active
  class:lowlife={lowLife}
  class:cmdmode={!!cmd}
  class:attacker={isAttacker}
  data-player={player.id}
  style:--pc={player.color}
  style:--art={player.image ? `url("${artCrop(player.image)}")` : 'none'}
  aria-label="{player.name}, {player.life} life"
>
  <div class="bg" aria-hidden="true"></div>

  <header>
    <span class="tokens">
      {#if s.monarch === player.id}
        <button class="token" title="Monarch — tap to choose who gets it, or drag it to a player" aria-label="Monarch token" onpointerdown={(e) => dragToken('monarch', e)}>♛</button>
      {/if}
      {#if s.initiative === player.id}
        <button class="token" title="Initiative — tap to choose who gets it, or drag it to a player" aria-label="Initiative token" onpointerdown={(e) => dragToken('initiative', e)}>⚔</button>
      {/if}
      {#if s.blessing.includes(player.id)}<span class="token static" title="City's blessing">✦</span>{/if}
    </span>
    {#if clockOn}
      <span class="clock num" class:low={(player.clockMs ?? 0) < 60000}>{fmtClock(player.clockMs ?? 0)}</span>
    {/if}
  </header>

  <button class="hit minus" aria-label={cmd && !isAttacker ? `Less damage from ${attacker?.name}` : 'Lose life (hold for 10)'} onpointerdown={(e) => down(-1, e)} onpointermove={move} onpointerup={up} onpointercancel={clearPress} oncontextmenu={(e) => e.preventDefault()}>
    <span>−</span>
  </button>
  <button class="hit plus" aria-label={cmd && !isAttacker ? `More damage from ${attacker?.name}` : 'Gain life (hold for 10)'} onpointerdown={(e) => down(1, e)} onpointermove={move} onpointerup={up} onpointercancel={clearPress} oncontextmenu={(e) => e.preventDefault()}>
    <span>+</span>
  </button>

  <div class="life" aria-live="polite">
    {#if cmd && isAttacker}
      <span class="mode-title">Dealing commander damage</span>
      <span class="small mode-sub">Tap the players you hit</span>
      <button class="done" onclick={() => (game.cmdFrom = null)}><Icon name="check" size={18} /> Done</button>
    {:else if cmd}
      <span class="mode-title">From {attacker?.name}{attacker?.partner ? (cmd.partner ? ' · partner' : ' · commander') : ''}</span>
      <span class="num big" class:danger={cmdTaken >= CMD_LETHAL - 5}>{cmdTaken}</span>
      <span class="small mode-sub">Life {player.life}</span>
    {:else}
      <span class="num big">{player.life}</span>
      {#if showDelta}<span class="delta num" class:neg={showDelta < 0}>{showDelta > 0 ? `+${showDelta}` : showDelta}</span>{/if}
      {#if out}<span class="out-label">{out}</span>{/if}
    {/if}
  </div>

  <footer>
    <span class="name">{active ? '▶ ' : ''}{player.name}</span>
    {#if !cmd}
      {#if maxCmd}
        <button class="chip" class:danger={maxCmd >= CMD_LETHAL - 5} onclick={() => { picking = null; editing = { kind: 'cmd' }; }} aria-label="Commander damage taken, most {maxCmd}. Tap to edit">⚔ {maxCmd}</button>
      {/if}
      {#each badges as b (b.key)}
        <button class="chip" class:danger={b.danger} onclick={() => { picking = null; editing = b.edit; }} aria-label="{b.label} {b.value}. Tap to edit">{b.label} {b.value}</button>
      {/each}
      <button class="menu-btn" onclick={onopen} aria-label="Menu for {player.name}">
        <Icon name="more" size={18} />
      </button>
    {/if}
  </footer>

  {#if editing || picking}
    <div class="overlay" class:picker={!!picking} role="dialog" aria-label={picking ? 'Choose who gets the token' : 'Edit counter'}>
      {#if picking}
        <p class="ov-title">{picking === 'monarch' ? '♛ Who becomes the Monarch?' : '⚔ Who has the Initiative?'}</p>
        <div class="ov-list picks">
          {#each s.players as p (p.id)}
            <button class="pick" class:on={s[picking] === p.id} style:--dc={p.color} onclick={() => give(p.id)}>
              <span class="dot"></span><span class="pick-name">{p.name}</span>{#if s[picking] === p.id}<span class="small">has it</span>{/if}
            </button>
          {/each}
          <button class="pick none" onclick={() => give(null)}>Nobody</button>
          <button class="pick none cancel" onclick={closeOverlays}>Cancel</button>
        </div>
      {:else if editing?.kind === 'cmd'}
        <p class="ov-title">⚔ Commander damage taken</p>
        <div class="ov-list">
          {#each opponents as o (o.id)}
            {#each o.partner ? [0, 1] : [0] as k (k)}
              {@const v = player.cmd[`${o.id}:${k}`] ?? 0}
              <div class="ov-row" class:danger={v >= CMD_LETHAL - 5}>
                <span class="dot" style:--dc={o.color}></span>
                <span class="pick-name">{o.name}{o.partner ? (k ? ' · partner' : ' · cmdr') : ''}</span>
                <button class="step" aria-label="Less damage from {o.name}" onclick={() => game.changeCmd(player.id, o.id, k as 0 | 1, -1)}>−</button>
                <span class="val num">{v}</span>
                <button class="step" aria-label="More damage from {o.name}" onclick={() => game.changeCmd(player.id, o.id, k as 0 | 1, 1)}>+</button>
              </div>
            {/each}
          {/each}
        </div>
        <button class="ov-done" onclick={closeOverlays}><Icon name="check" size={16} /> Done</button>
      {:else if quick}
        <p class="ov-title">{quick.label}</p>
        <div class="ov-stepper">
          <button class="step big-step" aria-label="Less {quick.label}" onclick={() => bump(-1)}>−</button>
          <span class="val num ov-val">{quick.value}</span>
          <button class="step big-step" aria-label="More {quick.label}" onclick={() => bump(1)}>+</button>
        </div>
        {#if quick.note}<p class="small ov-note">{quick.note}</p>{/if}
        <div class="ov-actions">
          <button class="ov-clear" onclick={clearQuick}>Clear</button>
          <button class="ov-done" onclick={closeOverlays}><Icon name="check" size={16} /> Done</button>
        </div>
      {/if}
    </div>
  {/if}
</section>

<style>
  .panel {
    position: relative;
    container-type: size;
    display: grid;
    grid-template-rows: auto 1fr auto;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    border-radius: 14px;
    background: var(--panel);
    color: #fff;
    isolation: isolate;
    user-select: none;
    -webkit-user-select: none;
    touch-action: none;
    transition: box-shadow 0.25s ease;
  }
  /* The panel fills its table cell; sideways panels swap width and height and
     turn about the cell's centre. */
  .panel {
    position: absolute;
    inset: 0;
  }
  .panel.rot180 {
    transform: rotate(180deg);
  }
  .panel.sideways {
    inset: auto;
    left: 50%;
    top: 50%;
    width: 100cqh;
    height: 100cqw;
  }
  .panel.rot90 {
    transform: translate(-50%, -50%) rotate(90deg);
  }
  .panel.rot270 {
    transform: translate(-50%, -50%) rotate(270deg);
  }
  .bg {
    position: absolute;
    inset: 0;
    z-index: -1;
    background:
      linear-gradient(180deg, rgba(10, 6, 16, 0.2), rgba(10, 6, 16, 0.72)),
      var(--art) center 28% / cover no-repeat,
      linear-gradient(160deg, color-mix(in srgb, var(--pc) 70%, #1a1622), color-mix(in srgb, var(--pc) 25%, #120e18));
  }
  .panel.active {
    box-shadow: inset 0 0 0 3px var(--pc), 0 0 22px color-mix(in srgb, var(--pc) 55%, transparent);
  }
  .panel.out .bg {
    filter: grayscale(1) brightness(0.45);
  }
  .panel.lowlife::after {
    content: '';
    position: absolute;
    inset: 0;
    z-index: 0;
    pointer-events: none;
    border-radius: inherit;
    box-shadow: inset 0 0 40px 6px rgba(220, 40, 30, 0.65);
    animation: pulse 1.6s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.25;
    }
  }
  .panel.cmdmode:not(.attacker) .bg {
    filter: saturate(0.6) brightness(0.8);
  }
  .panel.attacker {
    box-shadow: inset 0 0 0 4px #f2d58a;
  }

  header,
  footer {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.5rem 0.7rem;
    pointer-events: none;
  }
  header {
    justify-content: space-between;
    min-height: 2.4rem;
  }
  header > *,
  footer > button,
  footer > .chip {
    pointer-events: auto;
  }
  .tokens {
    display: flex;
    gap: 0.3rem;
  }
  .token {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 50%;
    border: 2px solid #f2d58a;
    background: rgba(20, 14, 6, 0.75);
    color: #f2d58a;
    font-size: 1.05rem;
    cursor: grab;
    touch-action: none;
  }
  .token.static {
    border-style: dashed;
    cursor: default;
  }
  .clock {
    margin-left: auto;
    padding: 0.1rem 0.55rem;
    border-radius: 99px;
    background: rgba(0, 0, 0, 0.5);
    font-weight: 700;
    font-size: 0.9rem;
  }
  .clock.low {
    background: #b8432e;
  }
  footer {
    flex-wrap: wrap;
    justify-content: center;
  }
  .name {
    flex-basis: 100%;
    min-width: 0;
    text-align: center;
    font-weight: 700;
    font-size: clamp(0.8rem, 6cqmin, 1.1rem);
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.6);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip {
    min-height: 30px;
    padding: 0.1rem 0.6rem;
    border: 1px solid rgba(255, 255, 255, 0.28);
    border-radius: 99px;
    background: rgba(0, 0, 0, 0.45);
    color: #fff;
    font: inherit;
    font-size: 0.78rem;
    font-weight: 700;
    white-space: nowrap;
    cursor: pointer;
  }
  .chip:active {
    background: rgba(255, 255, 255, 0.2);
  }
  .chip.danger {
    background: #b8432e;
  }
  .menu-btn {
    display: grid;
    place-items: center;
    width: 40px;
    height: 34px;
    border: 1px solid rgba(255, 255, 255, 0.35);
    border-radius: 99px;
    background: rgba(0, 0, 0, 0.4);
    color: #fff;
    cursor: pointer;
  }

  .hit {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 50%;
    z-index: 1;
    border: 0;
    background: transparent;
    color: rgba(255, 255, 255, 0.5);
    font-size: clamp(1.4rem, 12cqmin, 2.6rem);
    cursor: pointer;
    display: flex;
    align-items: center;
    padding: 0 8%;
    touch-action: none;
  }
  .hit:active {
    background: rgba(255, 255, 255, 0.08);
  }
  .attacker .hit {
    display: none;
  }
  .minus {
    left: 0;
    justify-content: flex-start;
  }
  .plus {
    right: 0;
    justify-content: flex-end;
  }
  .life {
    position: relative;
    z-index: 2;
    display: grid;
    place-items: center;
    align-content: center;
    gap: 0.15rem;
    pointer-events: none;
    text-align: center;
    padding: 0 0.5rem;
  }
  .big {
    font-family: var(--display);
    font-size: clamp(2.6rem, 34cqmin, 9rem);
    line-height: 1;
    text-shadow: 0 2px 14px rgba(0, 0, 0, 0.55);
    color: #fff;
    background: none;
    border: 0;
    padding: 0 0.1em;
    pointer-events: none;
  }
  .compact .big {
    font-size: clamp(2.2rem, 30cqmin, 7rem);
  }
  .big.danger {
    color: #ff9a85;
  }
  .mode-title {
    font-weight: 700;
    font-size: clamp(0.8rem, 6cqmin, 1rem);
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.6);
  }
  .mode-sub {
    color: rgba(255, 255, 255, 0.75);
  }
  .done {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    margin-top: 0.4rem;
    min-height: 40px;
    padding: 0 1rem;
    border: 0;
    border-radius: 99px;
    background: #f2d58a;
    color: #14101b;
    font-weight: 700;
    pointer-events: auto;
    cursor: pointer;
  }
  .delta {
    position: absolute;
    top: 6%;
    right: 16%;
    font-weight: 700;
    font-size: clamp(0.9rem, 8cqmin, 1.6rem);
    color: #9fe3ae;
  }
  .delta.neg {
    color: #ffb4a6;
  }
  .out-label {
    margin-top: 0.2rem;
    padding: 0.1rem 0.6rem;
    border-radius: 99px;
    background: #b8432e;
    font-size: 0.8rem;
    font-weight: 700;
  }
  .life {
    grid-row: 2;
  }
  /* Very short panels (6 players turned sideways): float the header and footer
     over the edges so the life total keeps the full height. */
  @container (max-height: 160px) {
    header,
    footer {
      position: absolute;
      left: 0;
      right: 0;
      min-height: 0;
      padding: 0.25rem 0.45rem;
    }
    header {
      top: 0;
    }
    footer {
      bottom: 0;
      flex-wrap: nowrap;
      justify-content: flex-start;
      overflow: hidden;
    }
    .name {
      flex: 1 1 auto;
      flex-basis: auto;
      text-align: left;
      font-size: 0.8rem;
    }
    .token {
      width: 28px;
      height: 28px;
      font-size: 0.9rem;
    }
    .menu-btn {
      flex-shrink: 0;
      width: 34px;
      height: 28px;
    }
    .chip {
      min-height: 26px;
      flex-shrink: 0;
    }
    .life {
      grid-row: 1 / -1;
    }
    .big,
    .compact .big {
      font-size: clamp(2rem, 52cqh, 5rem);
    }
  }

  /* --- Quick editor / token picker overlay -------------------------------- */
  .overlay {
    position: absolute;
    inset: 0;
    z-index: 6;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 0.6rem;
    background: rgba(14, 10, 20, 0.9);
    backdrop-filter: blur(4px);
    touch-action: pan-y;
    overflow: auto;
  }
  .ov-title {
    margin: 0;
    font-weight: 700;
    font-size: clamp(0.85rem, 7cqmin, 1.1rem);
    text-align: center;
  }
  .ov-list {
    display: grid;
    gap: 0.3rem;
    width: min(100%, 340px);
    max-height: 100%;
    overflow: auto;
    min-height: 0;
  }
  .pick,
  .ov-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 40px;
    padding: 0 0.7rem;
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    background: rgba(255, 255, 255, 0.06);
    color: #fff;
    font: inherit;
    font-weight: 700;
    font-size: 0.88rem;
    text-align: left;
  }
  .pick {
    cursor: pointer;
  }
  .pick.on {
    border-color: #f2d58a;
    background: rgba(242, 213, 138, 0.18);
  }
  .pick.none {
    justify-content: center;
    color: rgba(255, 255, 255, 0.7);
  }
  .pick.cancel {
    border-color: transparent;
    background: #f2d58a;
    color: #14101b;
  }
  .ov-row {
    padding: 0 0.3rem 0 0.6rem;
  }
  .ov-row.danger .val {
    color: #ff8f7a;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--dc, #fff);
  }
  .pick .dot {
    background: var(--dc);
  }
  .pick-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .step {
    display: grid;
    place-items: center;
    width: 38px;
    height: 34px;
    flex-shrink: 0;
    border: 1px solid rgba(255, 255, 255, 0.22);
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.08);
    color: #fff;
    font-size: 1.2rem;
    cursor: pointer;
  }
  .step:active {
    background: rgba(255, 255, 255, 0.2);
  }
  .val {
    min-width: 2.2ch;
    text-align: center;
    font-weight: 700;
  }
  .ov-stepper {
    display: flex;
    align-items: center;
    gap: 0.8rem;
  }
  .big-step {
    width: clamp(44px, 18cqmin, 64px);
    height: clamp(44px, 18cqmin, 64px);
    border-radius: 14px;
    font-size: 1.6rem;
  }
  .ov-val {
    font-family: var(--display);
    font-size: clamp(2rem, 22cqmin, 4rem);
    line-height: 1;
  }
  .ov-note {
    margin: 0;
    color: rgba(255, 255, 255, 0.65);
  }
  .ov-actions {
    display: flex;
    gap: 0.5rem;
  }
  .ov-done,
  .ov-clear {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    min-height: 38px;
    padding: 0 1rem;
    border-radius: 99px;
    font: inherit;
    font-weight: 700;
    cursor: pointer;
  }
  .ov-done {
    border: 0;
    background: #f2d58a;
    color: #14101b;
  }
  .ov-clear {
    border: 1px solid rgba(255, 255, 255, 0.3);
    background: transparent;
    color: #fff;
  }
  @container (max-height: 160px) {
    .overlay {
      flex-direction: row;
      flex-wrap: wrap;
      align-content: safe center;
      gap: 0.3rem 0.4rem;
      padding: 0.3rem 0.5rem;
    }
    .ov-title {
      flex-basis: 100%;
      font-size: 0.8rem;
    }
    .ov-list {
      max-height: calc(100cqh - 3.6rem);
    }
    .ov-list.picks {
      display: flex;
      flex-wrap: wrap;
      justify-content: center;
      width: 100%;
      max-height: none;
    }
    .pick {
      min-height: 30px;
      padding: 0 0.55rem;
      font-size: 0.78rem;
    }
    .pick .small,
    .ov-note,
    .picker .ov-title {
      display: none;
    }
    .ov-row {
      min-height: 34px;
    }
    .step {
      height: 30px;
    }
    .big-step {
      width: 44px;
      height: 38px;
    }
    .ov-val {
      font-size: 2rem;
    }
    .ov-done,
    .ov-clear {
      min-height: 32px;
      padding: 0 0.8rem;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .panel.lowlife::after {
      animation: none;
      opacity: 0.6;
    }
  }
</style>
