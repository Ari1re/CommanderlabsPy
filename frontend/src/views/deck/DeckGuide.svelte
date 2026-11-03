<!--
How to play tab: a guide built from the deck itself (mulligans, early
turns, how it wins, key cards, combos).

Things to work on:
- With an AI model running, the guide could be written in prose from the
  same facts.
-->
<script lang="ts">
  import { api } from '../../lib/api';
  import { app } from '../../lib/store.svelte';
  import type { CardView, PlayGuide } from '../../lib/types';
  import Icon from '../../components/Icon.svelte';

  let { name, version }: { name: string; version: number } = $props();

  let guide = $state<PlayGuide | null>(null);
  let error = $state<string | null>(null);

  async function load() {
    try {
      guide = await api<PlayGuide>('guide', { name });
      error = null;
    } catch (err) {
      error = (err as Error).message;
    }
  }

  $effect(() => {
    version;
    load();
  });

  async function inspect(card: string) {
    try {
      app.open(await api<CardView>('card', { name: card }));
    } catch (err) {
      app.error(err);
    }
  }
</script>

{#if error}
  <p class="dim">{error}</p>
{:else if !guide}
  <p class="muted">Reading your deck…</p>
{:else}
  <div class="intro">
    <h2>How to play this deck</h2>
    <p class="lede">{guide.summary}</p>
  </div>

  <div class="sw">
    {#if guide.strengths.length}
      <section class="panel box good">
        <h3>What it does well</h3>
        <ul>{#each guide.strengths as s, i (i)}<li>{s}</li>{/each}</ul>
      </section>
    {/if}
    {#if guide.weaknesses.length}
      <section class="panel box weak">
        <h3>What to improve</h3>
        <ul>{#each guide.weaknesses as w, i (i)}<li>{w}</li>{/each}</ul>
        <a class="btn small" href={`#/deck/${encodeURIComponent(name)}/suggestions`}><Icon name="plus" size={15} /> See cards that help</a>
      </section>
    {/if}
  </div>

  {#each guide.sections as sec (sec.title)}
    <section class="part">
      <h3>{sec.title}</h3>
      {#if sec.text}<p class="dim">{sec.text}</p>{/if}
      <ul>{#each sec.points as p, i (i)}<li>{p}</li>{/each}</ul>
      {#if sec.cards.length}
        <div class="cards">
          {#each sec.cards as c (c)}
            <button class="chip" onclick={() => inspect(c)}>{c}</button>
          {/each}
        </div>
      {/if}
    </section>
  {/each}

  {#if !guide.has_combos}
    <p class="small muted">
      Tip: run <a href={`#/deck/${encodeURIComponent(name)}/combos`}>Combos</a> and this guide will include
      your Commander Spellbook win lines.
    </p>
  {/if}
  <p class="small muted">Want advice in your own words? Ask the <a href={`#/deck/${encodeURIComponent(name)}/agent`}>AI Agent</a>.</p>
{/if}

<style>
  .intro {
    display: grid;
    gap: 0.4rem;
    margin-bottom: 1.25rem;
  }
  .lede {
    font-size: 1.05rem;
    max-width: 70ch;
  }
  .sw {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 1.25rem;
    margin-bottom: 1.75rem;
    align-items: start;
  }
  .box {
    padding: 1rem 1.25rem;
    display: grid;
    gap: 0.5rem;
    align-content: start;
  }
  .good {
    border-left: 3px solid var(--ok);
  }
  .weak {
    border-left: 3px solid var(--danger);
  }
  .box .btn {
    justify-self: start;
  }
  ul {
    margin: 0;
    padding-left: 1.15rem;
    display: grid;
    gap: 0.3rem;
  }
  .part {
    display: grid;
    gap: 0.5rem;
    padding: 1.25rem 0;
    border-top: 1px solid var(--line-soft);
    max-width: 78ch;
  }
  .cards {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .chip {
    padding: 0.25rem 0.65rem;
    border-radius: 99px;
    border: 1px solid var(--line);
    background: var(--panel);
    cursor: pointer;
    font-size: 0.85rem;
  }
  .chip:hover {
    border-color: var(--brass);
    color: var(--brass-strong);
  }
</style>
