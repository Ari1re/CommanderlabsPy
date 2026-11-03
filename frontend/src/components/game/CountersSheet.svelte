<!--
A player's full-screen menu in the Game tracker, turned to face them:
counters (poison, commander tax, custom), commander damage from each
opponent, their name and the card art behind their panel.
-->
<script lang="ts">
  import { api } from '../../lib/api';
  import { COUNTERS, cmdLethalOf, game, type Player, type Rotation } from '../../lib/game.svelte';
  import type { CardView } from '../../lib/types';
  import { artCrop } from '../../lib/util';
  import Icon from '../Icon.svelte';

  // A player's menu, full screen and turned to face them.
  let {
    player,
    rotation = 0,
    onclose,
    onedit,
  }: { player: Player; rotation?: Rotation; onclose: () => void; onedit: () => void } = $props();

  let customName = $state('');
  let artQuery = $state('');
  let art = $state<CardView[]>([]);
  let artTimer: ReturnType<typeof setTimeout> | undefined;
  let s = $derived(game.state!);
  let opponents = $derived(s.players.filter((p) => p.id !== player.id));
  let lethal = $derived(cmdLethalOf(s));
  let CMD_LETHAL = $derived(lethal ?? 21);
  // Commander tax only matters in commander modes.
  let counters = $derived(lethal == null ? COUNTERS.filter((c) => c.id !== 'tax') : COUNTERS);

  function addCustom() {
    game.addCustom(player.id, customName);
    customName = '';
  }

  function searchArt() {
    clearTimeout(artTimer);
    const q = artQuery.trim();
    if (q.length < 2) {
      art = [];
      return;
    }
    artTimer = setTimeout(async () => {
      try {
        art = (await api<CardView[]>('search_cards', { query: q, limit: 18 })).filter((c) => c.image_uri);
      } catch {
        art = [];
      }
    }, 250);
  }

  function dealDamage(partner: 0 | 1) {
    game.cmdFrom = { player: player.id, partner };
    onclose();
  }
</script>

<div class="sheet rot{rotation}" class:sideways={rotation === 90 || rotation === 270} role="dialog" aria-modal="true" aria-label="Menu for {player.name}" style:--pc={player.color}>
  <div class="sheet-head">
    <b>{player.name}</b>
    <span class="num life-mini">{player.life} life</span>
    <button class="close" onclick={onclose} aria-label="Done"><Icon name="check" size={18} /> Done</button>
  </div>

  <div class="rows">
    <div class="actions">
      {#if lethal != null}
      <button class="action primary" onclick={() => dealDamage(0)}>⚔ Deal commander damage</button>
      {/if}
      <button class="action" onclick={() => { onedit(); onclose(); }}>Set life total</button>
      {#if player.partner && lethal != null}
        <button class="action" onclick={() => dealDamage(1)}>⚔ With partner</button>
      {/if}
      {#if s.turns}
        <button class="action" onclick={() => { game.startTurns(player.id); onclose(); }}>▶ Make it my turn</button>
      {/if}
      <button class="action" onclick={() => game.concede(player.id)}>{player.conceded ? 'Rejoin game' : 'Concede'}</button>
    </div>

    {#if lethal != null}
    <p class="group">Commander damage taken ({lethal} loses)</p>
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
    {/if}

    <p class="group">Counters</p>
    {#each counters as c (c.id)}
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
    <p class="hint">Tap the ♛ or ⚔ token on a panel to choose who gets it, or drag it to another player.</p>

    <p class="group">My panel</p>
    <div class="toggles">
      <button onclick={() => game.rotatePlayer(player.id)}>⟳ Rotate my panel</button>
      {#if player.rotation != null}
        <button onclick={() => game.rotatePlayer(player.id, null)}>Follow the table layout</button>
      {/if}
    </div>
    <p class="hint">Turns your panel a quarter turn so it faces where you sit.</p>

    <p class="group">Background art</p>
    <input class="search" bind:value={artQuery} oninput={searchArt} placeholder="Search any card for its art" aria-label="Search card art" />
    {#if art.length}
      <div class="art">
        {#each art as c (c.oracle_id)}
          <button class="art-pick" class:on={player.image === c.image_uri} onclick={() => game.setImage(player.id, c.image_uri)} style:background-image={`url("${artCrop(c.image_uri)}")`} aria-label="Use {c.name} art">
            <span>{c.name}</span>
          </button>
        {/each}
      </div>
    {/if}
    {#if player.image}
      <button class="link" onclick={() => game.setImage(player.id, '')}>Remove background art</button>
    {/if}
  </div>
</div>

<style>
  .sheet {
    position: fixed;
    inset: 0;
    z-index: 74;
    display: grid;
    grid-template-rows: auto 1fr;
    background: #120d1a;
    color: #fff;
    padding: var(--safe-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left);
    border-top: 4px solid var(--pc);
  }
  .sheet.rot180 {
    transform: rotate(180deg);
  }
  /* Sideways: swap the screen's width and height, then turn about the centre. */
  .sheet.sideways {
    inset: auto;
    left: 50%;
    top: 50%;
    width: 100vh;
    width: 100dvh;
    height: 100vw;
    height: 100dvw;
    padding: 0;
  }
  .sheet.rot90 {
    transform: translate(-50%, -50%) rotate(90deg);
  }
  .sheet.rot270 {
    transform: translate(-50%, -50%) rotate(270deg);
  }
  .sheet-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    max-width: 560px;
    margin: 0 auto;
    padding: 0.8rem 1rem 0.6rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.12);
  }
  .sheet-head b {
    font-size: 1.15rem;
  }
  .life-mini {
    flex: 1;
    color: rgba(255, 255, 255, 0.65);
  }
  .close {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    min-height: 40px;
    padding: 0 0.9rem;
    border: 0;
    border-radius: 99px;
    background: var(--pc);
    color: #14101b;
    font-weight: 700;
    cursor: pointer;
  }
  .rows {
    width: 100%;
    max-width: 560px;
    margin: 0 auto;
    overflow: auto;
    padding: 0.5rem 1rem 1.5rem;
    overscroll-behavior: contain;
  }
  .actions {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 0.5rem;
    margin-bottom: 0.4rem;
  }
  .action {
    min-height: 46px;
    padding: 0 0.8rem;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.22);
    background: rgba(255, 255, 255, 0.06);
    color: #fff;
    font-weight: 700;
    font-size: 0.92rem;
    cursor: pointer;
  }
  .action.primary {
    background: var(--pc);
    border-color: var(--pc);
    color: #14101b;
  }
  .group {
    margin: 1rem 0 0.25rem;
    font-size: 0.78rem;
    font-weight: 700;
    color: rgba(255, 255, 255, 0.55);
  }
  .hint {
    font-size: 0.75rem;
    color: rgba(255, 255, 255, 0.45);
    margin: 0.25rem 0 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 54px;
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
    font-size: 0.95rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .val {
    min-width: 2.2ch;
    text-align: center;
    font-weight: 700;
    font-size: 1.1rem;
  }
  .step {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    flex-shrink: 0;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.06);
    color: #fff;
    font-size: 1.3rem;
    cursor: pointer;
  }
  .step:active {
    background: rgba(255, 255, 255, 0.16);
  }
  .step.rm {
    width: 34px;
    border-color: transparent;
    color: rgba(255, 255, 255, 0.55);
  }
  .add input,
  .search {
    flex: 1;
    min-width: 0;
    width: 100%;
    min-height: 44px;
    padding: 0 0.75rem;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    background: rgba(0, 0, 0, 0.3);
    color: #fff;
    font-size: 0.95rem;
  }
  .toggles {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .toggles button {
    min-height: 40px;
    padding: 0 0.85rem;
    border-radius: 99px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    background: transparent;
    color: #fff;
    font-weight: 700;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .toggles button[aria-pressed='true'] {
    background: var(--pc);
    border-color: var(--pc);
    color: #14101b;
  }
  .art {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 0.5rem;
    margin-top: 0.6rem;
  }
  .art-pick {
    position: relative;
    aspect-ratio: 4 / 3;
    border-radius: 10px;
    border: 2px solid transparent;
    background: #2a2236 center / cover;
    cursor: pointer;
    overflow: hidden;
  }
  .art-pick.on {
    border-color: var(--pc);
  }
  .art-pick span {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    padding: 0.2rem 0.4rem;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    font-size: 0.72rem;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .link {
    margin-top: 0.6rem;
    padding: 0;
    border: 0;
    background: none;
    color: rgba(255, 255, 255, 0.65);
    text-decoration: underline;
    cursor: pointer;
  }
</style>
