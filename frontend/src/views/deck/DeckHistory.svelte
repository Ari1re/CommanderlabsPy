<script lang="ts">
  import { api } from '../../lib/api';
  import { app } from '../../lib/store.svelte';
  import type { Deck, Version } from '../../lib/types';
  import Icon from '../../components/Icon.svelte';
  import Modal from '../../components/Modal.svelte';

  let { name, version, onchange }: { name: string; version: number; onchange: (d: Deck) => void } = $props();

  let versions = $state<Version[] | null>(null);
  let viewing = $state<{ v: Version; text: string } | null>(null);
  let confirm = $state<Version | null>(null);
  let expanded = $state<Set<number>>(new Set());

  async function load() {
    try {
      versions = await api<Version[]>('versions', { name });
    } catch (err) {
      app.error(err);
    }
  }

  $effect(() => {
    version;
    load();
  });

  function when(ms: number) {
    const d = new Date(ms);
    const today = new Date();
    const sameDay = d.toDateString() === today.toDateString();
    return sameDay
      ? `Today, ${d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}`
      : d.toLocaleString([], { month: 'short', day: 'numeric', year: d.getFullYear() === today.getFullYear() ? undefined : 'numeric', hour: 'numeric', minute: '2-digit' });
  }

  async function view(v: Version) {
    try {
      const res = await api<{ text: string }>('version_text', { name, id: v.id });
      viewing = { v, text: res.text };
    } catch (err) {
      app.error(err);
    }
  }

  async function restore(v: Version) {
    try {
      onchange(await api<Deck>('restore_version', { name, id: v.id }));
      confirm = null;
      viewing = null;
      app.toast('Restored. The version you replaced is still in the history.', 'success');
    } catch (err) {
      app.error(err);
    }
  }

  function toggle(id: number) {
    const next = new Set(expanded);
    next.has(id) ? next.delete(id) : next.add(id);
    expanded = next;
  }
</script>

<div class="head">
  <h2>Deck history</h2>
  <p class="dim small">Every change is saved. Restore any version; the current one stays in the history, so nothing is lost.</p>
</div>

{#if versions === null}
  <p class="muted">Loading history…</p>
{:else}
  <ol class="timeline">
    {#each versions as v, i (v.id)}
      {@const first = i === versions.length - 1}
      {@const changes = first ? [] : v.changes}
      {@const shown = expanded.has(v.id) ? changes : changes.slice(0, 6)}
      <li class:current={i === 0}>
        <span class="dot" aria-hidden="true"></span>
        <div class="entry">
          <div class="row">
            <b class="msg">{v.message}</b>
            {#if i === 0}<span class="badge brass">Current</span>{/if}
            <span class="spacer"></span>
            <span class="small muted">{when(v.timestamp_ms)}</span>
          </div>
          {#if changes.length}
            <ul class="changes small">
              {#each shown as c (c.zone + c.name)}
                <li class:add={c.delta > 0} class:remove={c.delta < 0}>
                  <span class="sign num">{c.delta > 0 ? `+${c.delta}` : c.delta}</span>
                  {c.name}{c.zone === 'sideboard' ? ' (sideboard)' : c.zone === 'commander' ? ' (commander)' : ''}
                </li>
              {/each}
            </ul>
            {#if changes.length > 6}
              <button class="link small" onclick={() => toggle(v.id)}>
                {expanded.has(v.id) ? 'Show fewer' : `Show all ${changes.length} changes`}
              </button>
            {/if}
          {/if}
          <div class="row tools">
            <span class="small muted">{v.cards} cards</span>
            <span class="spacer"></span>
            <button class="btn small ghost" onclick={() => view(v)}>View list</button>
            {#if i !== 0}
              <button class="btn small" onclick={() => (confirm = v)}><Icon name="refresh" size={14} /> Restore</button>
            {/if}
          </div>
        </div>
      </li>
    {/each}
  </ol>
{/if}

{#if viewing}
  <Modal title={viewing.v.message} onclose={() => (viewing = null)} wide>
    <p class="small muted">{when(viewing.v.timestamp_ms)} · {viewing.v.cards} cards</p>
    <textarea class="input" rows="18" readonly value={viewing.text}></textarea>
    {#snippet footer()}
      <button class="btn ghost" onclick={() => (viewing = null)}>Close</button>
      {#if versions && viewing && versions[0].id !== viewing.v.id}
        <button class="btn primary" onclick={() => restore(viewing!.v)}>Restore this version</button>
      {/if}
    {/snippet}
  </Modal>
{/if}

{#if confirm}
  <Modal title="Restore this version?" onclose={() => (confirm = null)}>
    <p class="dim">
      Your deck goes back to “{confirm.message}” from {when(confirm.timestamp_ms)}. The current version is kept
      in the history, so you can switch back.
    </p>
    {#snippet footer()}
      <button class="btn ghost" onclick={() => (confirm = null)}>Cancel</button>
      <button class="btn primary" onclick={() => restore(confirm!)}>Restore</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .head {
    display: grid;
    gap: 0.25rem;
    margin-bottom: 1.25rem;
  }
  .timeline {
    list-style: none;
    margin: 0;
    padding: 0;
    max-width: 52rem;
  }
  .timeline > li {
    position: relative;
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr);
    gap: 0.75rem;
    padding-bottom: 1rem;
  }
  .timeline > li::before {
    content: '';
    position: absolute;
    left: 11px;
    top: 18px;
    bottom: -2px;
    width: 2px;
    background: var(--line-soft);
  }
  .timeline > li:last-child::before {
    display: none;
  }
  .dot {
    width: 12px;
    height: 12px;
    margin: 6px 0 0 6px;
    border-radius: 50%;
    background: var(--line);
    position: relative;
  }
  .current .dot {
    background: var(--brass);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--brass) 25%, transparent);
  }
  .entry {
    display: grid;
    gap: 0.4rem;
    padding: 0.8rem 1rem;
    background: var(--panel);
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
  }
  .changes {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.1rem;
  }
  .sign {
    display: inline-block;
    width: 2.2rem;
    font-weight: 700;
  }
  .add .sign {
    color: var(--ok);
  }
  .remove .sign {
    color: var(--danger);
  }
  .remove {
    color: var(--text-2);
  }
  .tools {
    margin-top: 0.2rem;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--brass-strong);
    cursor: pointer;
    justify-self: start;
  }
  textarea {
    font-size: 0.9rem;
  }
</style>
