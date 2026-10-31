<script lang="ts">
  import { COUNTERS, CMD_LETHAL, game, type Player } from '../../lib/game.svelte';
  import Icon from '../Icon.svelte';

  // Full-screen counters for one player, turned to face them.
  let { player, flipped = false, onclose }: { player: Player; flipped?: boolean; onclose: () => void } = $props();

  let customName = $state('');
  let s = $derived(game.state!);
  let opponents = $derived(s.players.filter((p) => p.id !== player.id));

  function addCustom() {
    game.addCustom(player.id, customName);
    customName = '';
  }
</script>

    <div class="sheet" class:flipped role="dialog" aria-modal="true" aria-label="Counters for {player.name}" style:--pc={player.color}>
      <div class="sheet-head">
        <b>{player.name}</b>
        <span class="num life-mini">{player.life} life</span>
        <button class="close" onclick={onclose} aria-label="Done"><Icon name="check" size={18} /> Done</button>
      </div>
      <div class="rows">
        <p class="group">Commander damage taken</p>
        {#each opponents as o (o.id)}
          {#each o.partner ? [0, 1] : [0] as k (k)}
            {@const v = player.cmd[`${o.id}:${k}`] ?? 0}
            <div class="row" class:danger={v >= CMD_LETHAL - 5}>
              <span class="dot" style:background={o.color}></span>
              <span class="lbl">{o.name}{o.partner ? (k ? ' · partner' : ' · commander') : ''}</span>
              <button class="step" aria-label="Less damage from {o.name}" onclick={() => game.changeCmd(player.id, o.id, k as 0 | 1, -1)}>−</button>
              <span class="val num">{v}</span>
              <button class="step" aria-label="More damage from {o.name}" onclick={() => game.changeCmd(player.id, o.id, k as 0 | 1, 1)}>+</button>
            </div>
          {/each}
        {/each}
        {#if s.cmdLowersLife}<p class="hint">Commander damage also lowers life.</p>{/if}

        <p class="group">Counters</p>
        {#each COUNTERS as c (c.id)}
          {@const v = player.counters[c.id] ?? 0}
          <div class="row" class:danger={c.lethal != null && v >= c.lethal - 2}>
            <span class="lbl">{c.label}{c.id === 'tax' && v ? ` (+${v * 2})` : ''}</span>
            <button class="step" aria-label="Less {c.label}" onclick={() => game.changeCounter(player.id, c.id, -1)}>−</button>
            <span class="val num">{v}</span>
            <button class="step" aria-label="More {c.label}" onclick={() => game.changeCounter(player.id, c.id, 1)}>+</button>
          </div>
        {/each}
        {#each player.custom as c, i (c.name)}
          <div class="row">
            <span class="lbl">{c.name}</span>
            <button class="step" aria-label="Less {c.name}" onclick={() => game.changeCustom(player.id, i, -1)}>−</button>
            <span class="val num">{c.value}</span>
            <button class="step" aria-label="More {c.name}" onclick={() => game.changeCustom(player.id, i, 1)}>+</button>
            <button class="step rm" aria-label="Remove {c.name}" onclick={() => game.removeCustom(player.id, i)}><Icon name="close" size={14} /></button>
          </div>
        {/each}
        <form class="row add" onsubmit={(e) => { e.preventDefault(); addCustom(); }}>
          <input bind:value={customName} placeholder="Add a counter (e.g. Storm)" aria-label="New counter name" />
          <button class="step" disabled={!customName.trim()} aria-label="Add counter">+</button>
        </form>

        <p class="group">Designations</p>
        <div class="toggles">
          <button aria-pressed={s.monarch === player.id} onclick={() => game.setDesignation('monarch', player.id)}>♛ Monarch</button>
          <button aria-pressed={s.initiative === player.id} onclick={() => game.setDesignation('initiative', player.id)}>⚔ Initiative</button>
          <button aria-pressed={s.blessing.includes(player.id)} onclick={() => game.toggleBlessing(player.id)}>✦ City's blessing</button>
        </div>
      </div>
    </div>

<style>
  .sheet {
    position: absolute;
    inset: 0;
    z-index: 3;
    display: grid;
    grid-template-rows: auto 1fr;
    background: #120d1a;
  }
  .sheet-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.6rem 0.4rem 0.8rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.12);
  }
  .life-mini {
    color: rgba(255, 255, 255, 0.65);
    font-size: 0.85rem;
    flex: 1;
  }
  .close {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    min-height: 34px;
    padding: 0 0.7rem;
    border: 0;
    border-radius: 99px;
    background: var(--pc);
    color: #14101b;
    font-weight: 700;
    cursor: pointer;
  }
  .rows {
    overflow: auto;
    padding: 0.2rem 0.6rem 0.8rem;
    overscroll-behavior: contain;
  }
  .group {
    margin: 0.7rem 0 0.25rem;
    font-size: 0.75rem;
    font-weight: 700;
    color: rgba(255, 255, 255, 0.55);
  }
  .hint {
    font-size: 0.72rem;
    color: rgba(255, 255, 255, 0.45);
    margin: 0.15rem 0 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-height: 40px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
  }
  .row.danger .val {
    color: #ff8f7a;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .lbl {
    flex: 1;
    min-width: 0;
    font-size: 0.86rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .val {
    min-width: 2ch;
    text-align: center;
    font-weight: 700;
    font-size: 1.05rem;
  }
  .step {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    flex-shrink: 0;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.06);
    color: #fff;
    font-size: 1.15rem;
    cursor: pointer;
  }
  .step:active {
    background: rgba(255, 255, 255, 0.16);
  }
  .step.rm {
    width: 30px;
    border-color: transparent;
    color: rgba(255, 255, 255, 0.55);
  }
  .add input {
    flex: 1;
    min-width: 0;
    min-height: 36px;
    padding: 0 0.6rem;
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    background: rgba(0, 0, 0, 0.3);
    color: #fff;
    font-size: 0.86rem;
  }
  .toggles {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .toggles button {
    min-height: 34px;
    padding: 0 0.7rem;
    border-radius: 99px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    background: transparent;
    color: #fff;
    font-weight: 700;
    font-size: 0.8rem;
    cursor: pointer;
  }
  .toggles button[aria-pressed='true'] {
    background: var(--pc);
    border-color: var(--pc);
    color: #14101b;
  }
  .sheet {
    position: fixed;
    inset: 0;
    z-index: 74;
    color: #fff;
    padding: env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left);
    border-top: 4px solid var(--pc);
  }
  .sheet.flipped {
    transform: rotate(180deg);
  }
  .rows {
    width: 100%;
    max-width: 560px;
    margin: 0 auto;
    padding-inline: 1rem;
  }
  .sheet-head {
    width: 100%;
    max-width: 560px;
    margin: 0 auto;
    padding: 0.8rem 1rem 0.6rem;
  }
  .sheet-head b {
    font-size: 1.15rem;
  }
  .life-mini {
    font-size: 1rem;
  }
  .lbl {
    font-size: 0.95rem;
  }
  .step {
    width: 44px;
    height: 44px;
    font-size: 1.3rem;
  }
  .row {
    min-height: 52px;
  }
</style>
