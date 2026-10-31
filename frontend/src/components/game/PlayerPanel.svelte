<script lang="ts">
  import { onDestroy } from 'svelte';
  import { COUNTERS, CMD_LETHAL, game, isOut, type Player } from '../../lib/game.svelte';
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
    stopRepeat();
  });

  let s = $derived(game.state!);
  let opponents = $derived(s.players.filter((p) => p.id !== player.id));
  let out = $derived(isOut(player));
  let delta = $derived(game.deltas[player.id]);
  let showDelta = $derived(delta && now - delta.at < 1800 ? delta.value : 0);
  let cmdTotal = $derived(Object.values(player.cmd).reduce((a, b) => a + b, 0));
  let maxCmd = $derived(Math.max(0, ...Object.values(player.cmd)));
  let badges = $derived(
    [
      ...COUNTERS.filter((c) => player.counters[c.id]).map((c) => ({ label: c.label, value: c.id === 'tax' ? `+${player.counters[c.id] * 2}` : player.counters[c.id], danger: c.lethal != null && player.counters[c.id] >= c.lethal - 2 })),
      ...player.custom.filter((c) => c.value).map((c) => ({ label: c.name, value: c.value, danger: false })),
    ],
  );

  // Hold to repeat: first change on press, then repeat after a pause.
  let repeatTimer: ReturnType<typeof setTimeout> | undefined;
  let repeatInterval: ReturnType<typeof setInterval> | undefined;
  function startRepeat(by: number, e: PointerEvent) {
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    game.changeLife(player.id, by);
    navigator.vibrate?.(8);
    repeatTimer = setTimeout(() => {
      let ticks = 0;
      repeatInterval = setInterval(() => {
        ticks++;
        game.changeLife(player.id, ticks > 10 ? by * 5 : by);
      }, 140);
    }, 450);
  }
  function stopRepeat() {
    clearTimeout(repeatTimer);
    clearInterval(repeatInterval);
  }

</script>

<section
  class="panel"
  class:flipped
  class:out={!!out}
  class:compact
  class:first={s.firstPlayer === player.id}
  style:--pc={player.color}
  style:--art={player.image ? `url("${artCrop(player.image)}")` : 'none'}
  aria-label="{player.name}, {player.life} life"
>
  <div class="bg" aria-hidden="true"></div>

    <header>
      <span class="marks">
        {#if s.monarch === player.id}<span class="mark" title="Monarch">♛</span>{/if}
        {#if s.initiative === player.id}<span class="mark" title="Initiative">⚔</span>{/if}
        {#if s.blessing.includes(player.id)}<span class="mark" title="City's blessing">✦</span>{/if}
      </span>
    </header>

    <button class="hit minus" aria-label="Lose 1 life" onpointerdown={(e) => startRepeat(-1, e)} onpointerup={stopRepeat} onpointercancel={stopRepeat} onpointerleave={stopRepeat} oncontextmenu={(e) => e.preventDefault()}>
      <span>−</span>
    </button>
    <button class="hit plus" aria-label="Gain 1 life" onpointerdown={(e) => startRepeat(1, e)} onpointerup={stopRepeat} onpointercancel={stopRepeat} onpointerleave={stopRepeat} oncontextmenu={(e) => e.preventDefault()}>
      <span>+</span>
    </button>

    <div class="life" aria-live="polite">
      <span class="num">{player.life}</span>
      {#if showDelta}<span class="delta num" class:neg={showDelta < 0}>{showDelta > 0 ? `+${showDelta}` : showDelta}</span>{/if}
      {#if out}<span class="out-label">{out}</span>{/if}
    </div>

    <footer>
      <span class="name">{player.name}</span>
      {#if cmdTotal}
        <span class="chip" class:danger={maxCmd >= CMD_LETHAL - 5} title="Most commander damage from one commander">⚔ {maxCmd}</span>
      {/if}
      {#each badges as b (b.label)}
        <span class="chip" class:danger={b.danger}>{b.label} {b.value}</span>
      {/each}
      <button class="counters-btn" onclick={onopen} aria-label="Counters and commander damage for {player.name}">
        <Icon name="plus" size={16} /><span>Counters</span>
      </button>
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
    touch-action: manipulation;
  }
  .panel.flipped {
    transform: rotate(180deg);
  }
  .bg {
    position: absolute;
    inset: 0;
    z-index: -1;
    background:
      linear-gradient(180deg, rgba(10, 6, 16, 0.25), rgba(10, 6, 16, 0.7)),
      var(--art) center 30% / cover no-repeat,
      linear-gradient(160deg, color-mix(in srgb, var(--pc) 70%, #1a1622), color-mix(in srgb, var(--pc) 25%, #120e18));
  }
  .panel.first {
    box-shadow: inset 0 0 0 3px var(--pc);
  }
  .panel.out .bg {
    filter: grayscale(1) brightness(0.5);
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
  /* The name sits on the outer edge so the menu button at the table's
     centre never covers it. */
  header {
    justify-content: flex-end;
    min-height: 2.2rem;
  }
  footer .name {
    flex-basis: 100%;
    text-align: center;
  }
  .name {
    min-width: 0;
    font-weight: 700;
    font-size: clamp(0.8rem, 6cqmin, 1.1rem);
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.6);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mark {
    font-size: 1.1rem;
    color: #f2d58a;
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.6);
  }
  footer {
    flex-wrap: wrap;
    justify-content: center;
  }
  footer > * {
    pointer-events: auto;
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
  .counters-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    min-height: 32px;
    padding: 0 0.7rem;
    border: 1px solid rgba(255, 255, 255, 0.35);
    border-radius: 99px;
    background: rgba(0, 0, 0, 0.4);
    color: #fff;
    font-weight: 700;
    font-size: 0.8rem;
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
    color: rgba(255, 255, 255, 0.55);
    font-size: clamp(1.4rem, 12cqmin, 2.6rem);
    cursor: pointer;
    display: flex;
    align-items: center;
    padding: 0 8%;
  }
  .hit:active {
    background: rgba(255, 255, 255, 0.08);
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
    pointer-events: none;
  }
  .life .num {
    font-family: var(--display);
    font-size: clamp(2.6rem, 34cqmin, 9rem);
    line-height: 1;
    text-shadow: 0 2px 14px rgba(0, 0, 0, 0.55);
  }
  .compact .life .num {
    font-size: clamp(2.2rem, 30cqmin, 7rem);
  }
  .delta {
    position: absolute;
    top: 8%;
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

</style>
