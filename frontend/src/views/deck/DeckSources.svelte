<!--
Sources tab: how the deck was built (the deep learning generation
report: sources, play styles, evidence tiers, weak spots, why each card),
references the player added, and the research sources used.

Things to work on:
- Decks built before the report existed show no report; offer "explain
  this deck" that runs the evidence pass on an existing list.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api, runJob } from '../../lib/api';
  import { app } from '../../lib/store.svelte';
  import type { GenerationReport, Source } from '../../lib/types';
  import Icon from '../../components/Icon.svelte';

  let { name, report = null }: { name: string; report?: GenerationReport | null } = $props();

  const TIER: Record<string, string> = { synergy: 'Synergy', core: 'Commonly played', staple: 'Staple', could_use: 'Could use' };

  let sources = $state<Source[] | null>(null);
  let url = $state('');
  let busy = $state(false);

  onMount(async () => {
    try {
      sources = await api<Source[]>('sources', { name });
    } catch (err) {
      app.error(err);
      sources = [];
    }
  });

  async function add() {
    busy = true;
    try {
      sources = await runJob<Source[]>('add_reference', { name, url });
      url = '';
      app.toast('Reference added. The AI Agent and upgrades will use it.', 'success');
    } catch (err) {
      app.error(err);
    } finally {
      busy = false;
    }
  }

  async function remove(s: Source) {
    try {
      sources = await api<Source[]>('remove_reference', { name, url: s.url });
    } catch (err) {
      app.error(err);
    }
  }

  let mine = $derived((sources ?? []).filter((s) => s.user_provided));
  let auto = $derived((sources ?? []).filter((s) => !s.user_provided));
</script>

<div class="stack wrap">
  <section class="stack">
    <div>
      <h2>Your references</h2>
      <p class="dim small">
        Add decks or articles you want this deck to learn from. A Moxfield or Archidekt link is imported as a full
        list; other pages are read for strategy. The AI Agent and upgrades weigh these above automatic research.
      </p>
    </div>
    <form class="row" onsubmit={(e) => { e.preventDefault(); add(); }}>
      <input class="input grow" bind:value={url} placeholder="https://www.moxfield.com/decks/…" aria-label="Reference link" />
      <button class="btn primary" disabled={!url.trim() || busy}><Icon name="plus" size={16} /> {busy ? 'Adding…' : 'Add reference'}</button>
    </form>
    {#if mine.length}
      <ul class="sources">
        {#each mine as s (s.url)}
          <li>
            <div>
              <a href={s.url} target="_blank" rel="noreferrer">{s.title || s.url}</a>
              <p class="small muted">{s.is_deck ? 'Reference deck' : 'Reference page'} · {s.detail}</p>
            </div>
            <button class="btn ghost small icon" aria-label="Remove reference" onclick={() => remove(s)}><Icon name="trash" size={15} /></button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  {#if report}
    <section class="stack">
      <div>
        <h2>How this deck was built</h2>
        <p class="dim small">{report.engine}. Every source it asked, what each gave, and why each card is in.</p>
      </div>
      <ul class="sources">
        {#each report.sources as s (s.name)}
          <li>
            <div>
              <b>{s.name}</b>
              <p class="small muted">{s.detail}</p>
            </div>
            <span class="badge" class:ok={s.status === 'used'} class:warn={s.status === 'empty' || s.status === 'failed'}>{s.status}</span>
          </li>
        {/each}
      </ul>
      {#if report.themes.length}
        <p class="small">
          Play styles: {#each report.themes as t, i (t.name)}{i ? ' · ' : ''}<b>{t.name}</b> <span class="muted">(from {t.from})</span>{/each}
        </p>
      {/if}
      <p class="small">
        In the deck: <b>{report.deck_tiers.synergy}</b> synergy · <b>{report.deck_tiers.core}</b> commonly played ·
        <b>{report.deck_tiers.staple}</b> staples · <b>{report.deck_tiers.could_use}</b> could use · {report.deck_tiers.none} other
        <span class="muted">(from {report.pool_tiers.synergy + report.pool_tiers.core + report.pool_tiers.staple + report.pool_tiers.could_use} candidates with evidence)</span>
      </p>
      {#if report.weaknesses.length}
        <div class="weak panel">
          <b class="small">Weak spots</b>
          <ul class="small">{#each report.weaknesses as w, i (i)}<li>{w}</li>{/each}</ul>
        </div>
      {/if}
      <details>
        <summary class="small dim">Why each card ({report.picks.length})</summary>
        <ul class="picks small">
          {#each report.picks as p (p.card)}
            <li><b>{p.card}</b> <span class="muted">· {p.tier ? TIER[p.tier] : 'Filler'}</span>{#if p.reasons.length}: {p.reasons.join('; ')}{/if}</li>
          {/each}
        </ul>
      </details>
      {#if report.near_misses.length}
        <details>
          <summary class="small dim">Just missed the cut ({report.near_misses.length})</summary>
          <ul class="picks small">
            {#each report.near_misses as p (p.card)}<li><b>{p.card}</b>{#if p.reasons.length}: {p.reasons.join('; ')}{/if}</li>{/each}
          </ul>
        </details>
      {/if}
    </section>
  {/if}

  <section class="stack">
    <h2>Research sources</h2>
    {#if sources === null}
      <p class="muted">Loading…</p>
    {:else}
      <ul class="sources">
        {#each auto as s (s.url + s.title)}
          <li>
            <div>
              <a href={s.url} target="_blank" rel="noreferrer">{s.title}</a>
              <p class="small muted">{s.source}{s.detail ? ` · ${s.detail}` : ''}</p>
            </div>
            <Icon name="external" size={15} />
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

<style>
  .weak {
    padding: 0.6rem 0.9rem;
    border-style: dashed;
  }
  .weak ul,
  .picks {
    margin: 0.3rem 0 0;
    padding-left: 1.1rem;
    display: grid;
    gap: 0.25rem;
  }
  .badge.ok {
    color: var(--ok);
  }
  .badge.warn {
    color: var(--danger);
  }
  .wrap {
    max-width: 56rem;
    gap: 2rem;
  }
  .grow {
    flex: 1 1 300px;
    width: auto;
  }
  .sources {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
  }
  .sources li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 0.65rem 0;
    border-bottom: 1px solid var(--line-soft);
    color: var(--muted);
  }
  .sources a {
    display: inline-block;
    padding: 0.55rem 0;
    color: var(--text);
    text-decoration: none;
    font-weight: 700;
  }
  .sources a:hover {
    color: var(--brass-strong);
    text-decoration: underline;
  }
</style>
