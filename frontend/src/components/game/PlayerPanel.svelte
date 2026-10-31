<script lang="ts">
  import { onDestroy } from 'svelte';
  import { COUNTERS, CMD_LETHAL, LOW_LIFE, fmtClock, game, isOut, type Player } from '../../lib/game.svelte';
  import { artCrop } from '../../lib/util';
  import Icon from '../Icon.svelte';

  let {
    player,
    flipped = false,
    compact = false,
    onopen,
  }: { player: Player; flipped?: boolean; compact?: boolean; onopen: () => void } = $props();

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
  let badges = $derived([
    ...COUNTERS.filter((c) => player.counters[c.id]).map((c) => ({
      label: c.label,
      value: c.id === 'tax' ? `+${player.counters[c.id] * 2}` : String(player.counters[c.id]),
      danger: c.lethal != null && player.counters[c.id] >= c.lethal - 2,
    })),
    ...player.custom.filter((c) => c.value).map((c) => ({ label: c.name, value: String(c.value), danger: false })),
  ]);

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
    const dx = e.clientX - press.x;
    const dy = e.clientY - press.y;
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

  // --- Drag a token (Monarch / Initiative) to another player.
  function dragToken(kind: 'monarch' | 'initiative', e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    game.dragging = { kind, x: e.clientX, y: e.clientY };
    const moveFn = (ev: PointerEvent) => {
      if (game.dragging) game.dragging = { ...game.dragging, x: ev.clientX, y: ev.clientY };
    };
    const upFn = (ev: PointerEvent) => {
      window.removeEventListener('pointermove', moveFn);
      window.removeEventListener('pointerup', upFn);
      const target = document.elementsFromPoint(ev.clientX, ev.clientY).find((el) => (el as HTMLElement).dataset?.player);
      const id = target ? Number((target as HTMLElement).dataset.player) : null;
      if (id != null && id !== player.id) game.setDesignation(kind, id, false);
      game.dragging = null;
    };
    window.addEventListener('pointermove', moveFn);
    window.addEventListener('pointerup', upFn);
  }
</script>

<section
  class="panel"
  class:flipped
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
        <button class="token" title="Monarch — drag to another player" aria-label="Monarch token" onpointerdown={(e) => dragToken('monarch', e)}>♛</button>
      {/if}
      {#if s.initiative === player.id}
        <button class="token" title="Initiative — drag to another player" aria-label="Initiative token" onpointerdown={(e) => dragToken('initiative', e)}>⚔</button>
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
        <span class="chip" class:danger={maxCmd >= CMD_LETHAL - 5} title="Most commander damage from one commander">⚔ {maxCmd}</span>
      {/if}
      {#each badges as b (b.label)}
        <span class="chip" class:danger={b.danger}>{b.label} {b.value}</span>
      {/each}
      <button class="menu-btn" onclick={onopen} aria-label="Menu for {player.name}">
        <Icon name="more" size={18} />
      </button>
    {/if}
  </footer>
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
  .panel.flipped {
    transform: rotate(180deg);
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
    padding: 0.1rem 0.5rem;
    border-radius: 99px;
    background: rgba(0, 0, 0, 0.45);
    font-size: 0.78rem;
    font-weight: 700;
    white-space: nowrap;
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
  @media (prefers-reduced-motion: reduce) {
    .panel.lowlife::after {
      animation: none;
      opacity: 0.6;
    }
  }
</style>
