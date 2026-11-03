<!--
Cards tab: the deck editor. Add cards by search, change quantities, move
cards between the deck, sideboard and Considering, and manage tags
Moxfield-style: tag chips filter the list, Group by switches between
type, tag, mana value and color, checkboxes select cards for bulk
tag/untag/move/remove (`edit_deck_bulk`), and Auto-tag tags by role.
Tags are saved in the decklist as "#Tag".

Also like Moxfield:
- The add box takes "2x Sol Ring #Ramp #Mana": a quantity and tags.
- Drag cards (or every selected card) onto a group: onto Sideboard,
  Considering or Commander to move them; in Group by Tag, onto another tag
  to retag them (Untagged clears the tag). Desktop only; phones use
  select + the bulk bar.
- Keys on the card under the pointer or focused: + / − quantity, S
  sideboard, C Considering, M deck, Delete remove, X select, / add box,
  Esc clear selection.
- The Images view shows tags and has selection boxes too.
-->
<script lang="ts">
  import { api } from '../../lib/api';
  import { app } from '../../lib/store.svelte';
  import type { CardView, Deck, Entry, Zone } from '../../lib/types';
  import { entryImage, KIND_ORDER, money, price } from '../../lib/util';
  import Icon from '../../components/Icon.svelte';
  import Mana from '../../components/Mana.svelte';

  let { deck, onchange }: { deck: Deck; onchange: (d: Deck) => void } = $props();

  let view = $state<'list' | 'visual'>('list');
  let sort = $state<'cmc' | 'name'>('cmc');
  let groupBy = $state<'type' | 'tag' | 'cmc' | 'color'>('type');
  /** Show only cards with this tag. */
  let tagFilter = $state<string | null>(null);
  let textFilter = $state('');
  /** Selected cards, by key(): for bulk tagging and moving. */
  let selected = $state<Set<string>>(new Set());
  let bulkTag = $state('');
  let tagDraft = $state('');
  let autoTagging = $state(false);
  let hovered = $state<CardView | null>(null);
  let menuFor = $state<string | null>(null);

  // --- add-card search ----------------------------------------------------
  let query = $state('');
  let results = $state<CardView[]>([]);
  let active = $state(0);
  let timer: ReturnType<typeof setTimeout> | undefined;

  /** "2x Sol Ring #Ramp #Mana" → quantity 2, name "Sol Ring", tags. */
  function parseAdd(text: string) {
    let rest = text.trim();
    const tags: string[] = [];
    rest = rest.replace(/(^|\s)#([^#]+?)(?=\s+#|$)/g, (_m, _sp, t) => {
      tags.push(String(t).trim());
      return '';
    }).trim();
    const m = /^(\d+)\s*x?\s+(.+)$/i.exec(rest);
    return m ? { qty: Math.max(1, parseInt(m[1])), name: m[2].trim(), tags } : { qty: 1, name: rest, tags };
  }

  function search() {
    clearTimeout(timer);
    const q = parseAdd(query).name;
    if (q.length < 2) {
      results = [];
      return;
    }
    timer = setTimeout(async () => {
      try {
        results = await api<CardView[]>('search_cards', { query: q, limit: 12 });
        active = 0;
      } catch {
        results = [];
      }
    }, 200);
  }

  async function edit(args: Record<string, unknown>, message?: string) {
    try {
      const next = await api<Deck>('edit_deck', { name: deck.name, ...args });
      onchange(next);
      if (message) app.toast(message, 'success');
    } catch (err) {
      app.error(err);
    }
  }

  async function bulk(op: 'add_tag' | 'remove_tag' | 'move' | 'remove', extra: Record<string, unknown> = {}) {
    const cards = deck.entries.filter((e) => selected.has(key(e))).map((e) => ({ card: e.name, zone: e.zone }));
    if (!cards.length) return;
    try {
      const next = await api<Deck>('edit_deck_bulk', { name: deck.name, cards, op, ...extra });
      onchange(next);
      if (op === 'move' || op === 'remove') selected = new Set();
      app.toast(
        op === 'add_tag' ? `Tagged ${cards.length} card${cards.length === 1 ? '' : 's'} #${extra.tag}.`
          : op === 'remove_tag' ? `Removed #${extra.tag} from ${cards.length} card${cards.length === 1 ? '' : 's'}.`
          : op === 'move' ? `Moved ${cards.length} card${cards.length === 1 ? '' : 's'}.`
          : `Removed ${cards.length} card${cards.length === 1 ? '' : 's'}.`,
        'success',
      );
    } catch (err) {
      app.error(err);
    }
  }

  async function autoTag() {
    autoTagging = true;
    try {
      onchange(await api<Deck>('auto_tag', { name: deck.name }));
      app.toast('Tagged cards by what they do. Your own tags were kept.', 'success');
    } catch (err) {
      app.error(err);
    } finally {
      autoTagging = false;
    }
  }

  function setTags(e: Entry, tags: string[]) {
    edit({ action: 'tags', card: e.name, zone: e.zone, tags });
  }

  function toggleSelect(e: Entry) {
    const next = new Set(selected);
    const k = key(e);
    if (next.has(k)) next.delete(k);
    else next.add(k);
    selected = next;
  }

  /** Every tag in the deck, with how many cards carry it. */
  let allTags = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const e of deck.entries) for (const t of e.tags ?? []) counts.set(t, (counts.get(t) ?? 0) + e.quantity);
    return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  });

  async function add(card: CardView, zone: Zone = 'main') {
    const { qty, tags } = parseAdd(query);
    await edit({ action: 'add', card: card.name, zone, qty }, `Added ${qty > 1 ? `${qty} ` : ''}${card.name}${tags.length ? ` #${tags.join(' #')}` : ''}.`);
    if (tags.length) {
      const entry = deck.entries.find((e) => e.zone === zone && e.name.toLowerCase() === card.name.toLowerCase());
      const merged = [...new Set([...(entry?.tags ?? []), ...tags])];
      await edit({ action: 'tags', card: card.name, zone, tags: merged });
    }
    query = '';
    results = [];
  }

  // --- drag and drop ----------------------------------------------------------
  /** Cards being dragged: the dragged one, or every selected card if it's selected. */
  let dragging = $state<Entry[]>([]);
  let dragFrom = $state<string | null>(null);
  let dropTarget = $state<string | null>(null);

  function startDrag(ev: DragEvent, e: Entry, group: string) {
    dragging = selected.has(key(e)) ? deck.entries.filter((x) => selected.has(key(x))) : [e];
    dragFrom = group;
    ev.dataTransfer?.setData('text/plain', e.name);
    if (ev.dataTransfer) ev.dataTransfer.effectAllowed = 'move';
  }

  function endDrag() {
    dragging = [];
    dragFrom = null;
    dropTarget = null;
  }

  const ZONE_GROUPS: Record<string, Zone> = { Sideboard: 'sideboard', Considering: 'considering', Commander: 'commander' };

  /** What dropping on `group` would do, or null if nothing. */
  function dropAction(group: string): string | null {
    if (!dragging.length || group === dragFrom) return null;
    if (ZONE_GROUPS[group]) {
      if (group === 'Commander') return dragging.length === 1 && dragging[0].card?.commander_eligible && dragging[0].zone !== 'commander' ? 'Make commander' : null;
      return dragging.every((d) => d.zone === ZONE_GROUPS[group]) ? null : `Move to ${group}`;
    }
    if (dragging.some((d) => d.zone !== 'main')) return 'Move to the deck';
    if (groupBy === 'tag') return group === 'Untagged' ? `Remove #${dragFrom}` : `Tag #${group}`;
    return null;
  }

  async function drop(group: string) {
    const action = dropAction(group);
    const cards = dragging;
    const from = dragFrom;
    endDrag();
    if (!action) return;
    const list = cards.map((c) => ({ card: c.name, zone: c.zone }));
    const run = async (op: string, extra: Record<string, unknown>) =>
      onchange(await api<Deck>('edit_deck_bulk', { name: deck.name, cards: list, op, ...extra }));
    try {
      if (group === 'Commander') {
        await edit({ action: 'move', card: cards[0].name, zone: cards[0].zone, to: 'commander', qty: 1 }, `${cards[0].name} is now a commander.`);
        return;
      }
      if (ZONE_GROUPS[group]) await run('move', { to: ZONE_GROUPS[group] });
      else if (cards.some((c) => c.zone !== 'main')) await run('move', { to: 'main' });
      else if (groupBy === 'tag') {
        // Moving between tag groups swaps the tag, like Moxfield's categories.
        if (from && from !== 'Untagged' && !ZONE_GROUPS[from]) await run('remove_tag', { tag: from });
        if (group !== 'Untagged') await run('add_tag', { tag: group });
      }
      selected = new Set();
      app.toast(`${action}: ${cards.length === 1 ? cards[0].name : `${cards.length} cards`}.`, 'success');
    } catch (err) {
      app.error(err);
    }
  }

  // --- keyboard shortcuts -------------------------------------------------------
  /** The card under the pointer (or focused) for shortcuts. */
  let keyTarget = $state<Entry | null>(null);
  let addInput: HTMLInputElement | undefined = $state();

  function shortcut(ev: KeyboardEvent) {
    const t = ev.target as HTMLElement | null;
    if (ev.ctrlKey || ev.metaKey || ev.altKey || t?.closest('input, textarea, select, [contenteditable]')) return;
    if (ev.key === '/') {
      addInput?.focus();
      ev.preventDefault();
      return;
    }
    if (ev.key === 'Escape' && selected.size) {
      selected = new Set();
      return;
    }
    const e = keyTarget && deck.entries.find((x) => key(x) === key(keyTarget!));
    if (!e) return;
    const k = ev.key.toLowerCase();
    const move = (to: Zone, label: string) => e.zone !== to && edit({ action: 'move', card: e.name, zone: e.zone, to }, `Moved ${e.name} to ${label}.`);
    if ((k === '+' || k === '=') && e.zone !== 'commander') edit({ action: 'add', card: e.name, zone: e.zone });
    else if ((k === '-' || k === '_') && e.zone !== 'commander') edit({ action: 'remove', card: e.name, zone: e.zone, qty: 1 });
    else if (k === 's' && e.zone !== 'commander') move('sideboard', 'the sideboard');
    else if (k === 'c' && e.zone !== 'commander') move('considering', 'Considering');
    else if (k === 'm' && e.zone !== 'commander') move('main', 'the deck');
    else if (k === 'delete' || k === 'backspace') edit({ action: 'remove', card: e.name, zone: e.zone }, `Removed ${e.name}.`);
    else if (k === 'x') toggleSelect(e);
    else return;
    ev.preventDefault();
  }

  function onkey(e: KeyboardEvent) {
    if (!results.length) return;
    if (e.key === 'ArrowDown') {
      active = (active + 1) % results.length;
      e.preventDefault();
    } else if (e.key === 'ArrowUp') {
      active = (active - 1 + results.length) % results.length;
      e.preventDefault();
    } else if (e.key === 'Enter') {
      add(results[active]);
      e.preventDefault();
    } else if (e.key === 'Escape') {
      results = [];
    }
  }

  // --- grouping -------------------------------------------------------------
  interface Group {
    title: string;
    entries: Entry[];
    count: number;
  }

  function sorted(list: Entry[]) {
    return [...list].sort((a, b) =>
      sort === 'cmc'
        ? (a.card?.cmc ?? 99) - (b.card?.cmc ?? 99) || a.name.localeCompare(b.name)
        : a.name.localeCompare(b.name),
    );
  }

  const COLOR_NAME: Record<string, string> = { W: 'White', U: 'Blue', B: 'Black', R: 'Red', G: 'Green' };

  /** The group(s) a main-deck card belongs to under the current grouping. */
  function groupKeys(e: Entry): string[] {
    if (groupBy === 'tag') return e.tags?.length ? e.tags : ['Untagged'];
    if (groupBy === 'cmc') {
      if (e.card?.kind === 'Land') return ['Lands'];
      const v = Math.floor(e.card?.cmc ?? 0);
      return [v >= 7 ? '7+ mana' : `${v} mana`];
    }
    if (groupBy === 'color') {
      const id = e.card?.color_identity ?? '';
      if (e.card?.kind === 'Land') return ['Lands'];
      return [id.length === 0 ? 'Colorless' : id.length > 1 ? 'Multicolor' : COLOR_NAME[id] ?? id];
    }
    return [e.card?.kind ?? 'Other'];
  }

  function groupOrder(keys: string[]): string[] {
    if (groupBy === 'type') return KIND_ORDER.filter((k) => keys.includes(k));
    if (groupBy === 'cmc') return [...keys].sort((a, b) => (a === 'Lands' ? 1 : b === 'Lands' ? -1 : parseInt(a) - parseInt(b)));
    if (groupBy === 'color') return ['White', 'Blue', 'Black', 'Red', 'Green', 'Multicolor', 'Colorless', 'Lands'].filter((k) => keys.includes(k));
    // Tags: biggest first, untagged last.
    return [...keys].sort((a, b) => (a === 'Untagged' ? 1 : b === 'Untagged' ? -1 : (allTags.find((t) => t[0] === b)?.[1] ?? 0) - (allTags.find((t) => t[0] === a)?.[1] ?? 0)));
  }

  let visible = $derived(
    deck.entries.filter(
      (e) =>
        (!tagFilter || (e.tags ?? []).includes(tagFilter)) &&
        (!textFilter.trim() || `${e.name} ${e.card?.type_line ?? ''} ${(e.tags ?? []).join(' ')}`.toLowerCase().includes(textFilter.trim().toLowerCase())),
    ),
  );

  let groups = $derived.by<Group[]>(() => {
    const byKey = new Map<string, Entry[]>();
    const push = (k: string, e: Entry) => {
      if (!byKey.has(k)) byKey.set(k, []);
      byKey.get(k)!.push(e);
    };
    for (const e of visible) {
      if (e.zone === 'commander') push('Commander', e);
      else if (e.zone === 'sideboard') push('Sideboard', e);
      else if (e.zone === 'considering') push('Considering', e);
      else for (const k of groupKeys(e)) push(k, e);
    }
    const middle = groupOrder([...byKey.keys()].filter((k) => !['Commander', 'Sideboard', 'Considering'].includes(k)));
    return ['Commander', ...middle, 'Sideboard', 'Considering']
      .filter((k) => byKey.has(k))
      .map((k) => {
        const entries = sorted(byKey.get(k)!);
        return { title: k === 'Other' ? 'Unknown cards' : k, entries, count: entries.reduce((n, e) => n + e.quantity, 0) };
      });
  });

  let previewCard = $derived(hovered ?? deck.entries.find((e) => e.zone === 'commander')?.card ?? null);

  function key(e: Entry) {
    return `${e.zone}:${e.name}`;
  }
</script>

<!-- Clicks in a card menu's tag box keep the menu open. -->
<svelte:window onclick={(ev) => { if (!(ev.target as HTMLElement | null)?.closest('.menu-tags')) menuFor = null; }} onkeydown={shortcut} />

<div class="toolbar">
  <div class="add">
    <Icon name="search" size={17} />
    <input
      class="input"
      placeholder="Add a card (2x Sol Ring #Ramp)"
      bind:value={query}
      bind:this={addInput}
      oninput={search}
      onkeydown={onkey}
      aria-label="Search cards to add"
      autocomplete="off"
    />
    {#if results.length}
      <ul class="results" role="listbox">
        {#each results as c, i (c.oracle_id)}
          <li>
            <button class:active={i === active} onmousedown={() => add(c)} onmouseenter={() => (hovered = c)}>
              <span class="r-name">{c.name}</span>
              <Mana cost={c.mana_cost} size={14} />
              <span class="small muted r-type">{c.kind}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  <div class="segmented" role="group" aria-label="View">
    <button aria-pressed={view === 'list'} onclick={() => (view = 'list')}>List</button>
    <button aria-pressed={view === 'visual'} onclick={() => (view = 'visual')}>Images</button>
  </div>
  <div class="segmented" role="group" aria-label="Sort">
    <button aria-pressed={sort === 'cmc'} onclick={() => (sort = 'cmc')}>Mana value</button>
    <button aria-pressed={sort === 'name'} onclick={() => (sort = 'name')}>Name</button>
  </div>
  <label class="group-by small">Group by
    <select class="input" bind:value={groupBy}>
      <option value="type">Type</option>
      <option value="tag">Tag</option>
      <option value="cmc">Mana value</option>
      <option value="color">Color</option>
    </select>
  </label>
  <button class="btn ghost" onclick={autoTag} disabled={autoTagging}>
    <Icon name="sparkle" size={15} /> {autoTagging ? 'Tagging…' : 'Auto-tag'}
  </button>
</div>

<div class="filters">
  <input class="input filter" placeholder="Filter cards" bind:value={textFilter} aria-label="Filter cards by name, type or tag" />
  {#if allTags.length}
    <div class="tag-chips" role="group" aria-label="Filter by tag">
      {#each allTags as [t, n] (t)}
        <button class="chip" aria-pressed={tagFilter === t} onclick={() => (tagFilter = tagFilter === t ? null : t)}>#{t} <span class="muted num">{n}</span></button>
      {/each}
    </div>
  {:else}
    <span class="small muted">No tags yet. Auto-tag tags every card by its role (Ramp, Removal, Card Draw…), Game Changers, combo pieces and the deck's play styles; or tag cards from their menu.</span>
  {/if}
</div>

{#if selected.size}
  <div class="bulk panel" role="toolbar" aria-label="Selected cards">
    <b class="small">{selected.size} selected</b>
    <form class="row" onsubmit={(ev) => { ev.preventDefault(); if (bulkTag.trim()) bulk('add_tag', { tag: bulkTag.trim() }); }}>
      <input class="input small-input" list="deck-tags" placeholder="Tag" bind:value={bulkTag} aria-label="Tag to add or remove" />
      <button class="btn small" disabled={!bulkTag.trim()}>Add tag</button>
      <button type="button" class="btn small ghost" disabled={!bulkTag.trim()} onclick={() => bulk('remove_tag', { tag: bulkTag.trim() })}>Remove tag</button>
    </form>
    <span class="spacer"></span>
    <button class="btn small ghost" onclick={() => bulk('move', { to: 'main' })}>To deck</button>
    <button class="btn small ghost" onclick={() => bulk('move', { to: 'sideboard' })}>To sideboard</button>
    <button class="btn small ghost" onclick={() => bulk('move', { to: 'considering' })}>To Considering</button>
    <button class="btn small ghost danger" onclick={() => bulk('remove')}>Remove</button>
    <button class="btn small ghost" onclick={() => (selected = new Set())}>Clear</button>
  </div>
{/if}
<datalist id="deck-tags">{#each allTags as [t] (t)}<option value={t}></option>{/each}</datalist>

<div class="layout" class:visual={view === 'visual'}>
  <div class="groups">
    {#each groups as g (g.title)}
      <section
        class="group"
        class:drop-ok={dropTarget === g.title && !!dropAction(g.title)}
        role="group"
        aria-label={g.title}
        ondragover={(ev) => { if (dropAction(g.title)) { ev.preventDefault(); dropTarget = g.title; } }}
        ondragleave={(ev) => { if (!(ev.currentTarget as HTMLElement).contains(ev.relatedTarget as Node)) dropTarget = null; }}
        ondrop={(ev) => { ev.preventDefault(); drop(g.title); }}
      >
        <h3>{g.title} <span class="muted num">{g.count}</span>
          {#if dropTarget === g.title && dropAction(g.title)}<span class="drop-hint small">{dropAction(g.title)}</span>{/if}
        </h3>
        {#if view === 'list'}
          <ul class="rows">
            {#each g.entries as e (key(e))}
              <li
                class="entry"
                class:picked={selected.has(key(e))}
                class:dragging={dragging.some((d) => key(d) === key(e))}
                draggable="true"
                ondragstart={(ev) => startDrag(ev, e, g.title)}
                ondragend={endDrag}
                onmouseenter={() => { hovered = e.card ? { ...e.card, image_uri: entryImage(e) } : null; keyTarget = e; }}
                onfocusin={() => (keyTarget = e)}
              >
                <input type="checkbox" class="pick" class:on={selected.size > 0} checked={selected.has(key(e))} onchange={() => toggleSelect(e)} aria-label="Select {e.name}" />
                <span class="qty num">{e.quantity}</span>
                <button class="name" onclick={() => e.card && app.open(e.card, { deck: deck.name, entry: e })} class:unknown={!e.card}>
                  {e.name}
                  {#if e.card?.game_changer}<span class="gc" title="Game Changer">◆</span>{/if}
                  {#if e.set_code}<span class="set small muted">{e.set_code}</span>{/if}
                  {#if e.foil}<span class="foil-tag small">foil</span>{/if}
                  {#each (e.tags ?? []).slice(0, 2) as t (t)}<span class="tag small" class:active={tagFilter === t}>#{t}</span>{/each}
                  {#if (e.tags?.length ?? 0) > 2}<span class="tag small" title={e.tags!.slice(2).map((t) => `#${t}`).join(' ')}>+{e.tags!.length - 2}</span>{/if}
                </button>
                {#if e.card}<Mana cost={e.card.mana_cost} size={14} />{/if}
                <span class="price small num muted">{e.card ? money(price(e.card, app.currency), app.currency) : 'not found'}</span>
                <span class="tools">
                  {#if e.zone !== 'commander'}
                    <button class="btn ghost small icon" aria-label="Add one" onclick={() => edit({ action: 'add', card: e.name, zone: e.zone })}><Icon name="plus" size={15} /></button>
                    <button class="btn ghost small icon" aria-label="Remove one" onclick={() => edit({ action: 'remove', card: e.name, zone: e.zone, qty: 1 })}><Icon name="minus" size={15} /></button>
                  {/if}
                  <span class="menu-wrap">
                    <button class="btn ghost small icon" aria-label="More for {e.name}" onclick={(ev) => { ev.stopPropagation(); menuFor = menuFor === key(e) ? null : key(e); }}><Icon name="more" size={15} /></button>
                    {#if menuFor === key(e)}
                      <div class="menu" role="menu">
                        {#if e.zone === 'main'}
                          <button role="menuitem" onclick={() => edit({ action: 'move', card: e.name, zone: 'main', to: 'sideboard' }, `Moved ${e.name} to the sideboard.`)}>Move to sideboard</button>
                          <button role="menuitem" onclick={() => edit({ action: 'move', card: e.name, zone: 'main', to: 'considering' }, `Moved ${e.name} to Considering.`)}>Move to Considering</button>
                          {#if e.card?.commander_eligible}
                            <button role="menuitem" onclick={() => edit({ action: 'move', card: e.name, zone: 'main', to: 'commander', qty: 1 }, `${e.name} is now a commander.`)}>Make commander</button>
                          {/if}
                        {:else if e.zone === 'sideboard'}
                          <button role="menuitem" onclick={() => edit({ action: 'move', card: e.name, zone: 'sideboard', to: 'main' }, `Moved ${e.name} to the deck.`)}>Move to deck</button>
                          <button role="menuitem" onclick={() => edit({ action: 'move', card: e.name, zone: 'sideboard', to: 'considering' }, `Moved ${e.name} to Considering.`)}>Move to Considering</button>
                        {:else if e.zone === 'considering'}
                          <button role="menuitem" onclick={() => edit({ action: 'move', card: e.name, zone: 'considering', to: 'main' }, `Moved ${e.name} to the deck.`)}>Move to deck</button>
                          <button role="menuitem" onclick={() => edit({ action: 'move', card: e.name, zone: 'considering', to: 'sideboard' }, `Moved ${e.name} to the sideboard.`)}>Move to sideboard</button>
                        {:else}
                          <button role="menuitem" onclick={() => edit({ action: 'move', card: e.name, zone: 'commander', to: 'main' })}>Move to the 99</button>
                        {/if}
                        <div class="menu-tags" role="group" aria-label="Tags">
                          {#each e.tags ?? [] as t (t)}
                            <button class="tag removable small" aria-label="Remove tag {t}" onclick={() => setTags(e, (e.tags ?? []).filter((x) => x !== t))}>#{t} ×</button>
                          {/each}
                          <form onsubmit={(ev) => { ev.preventDefault(); if (tagDraft.trim()) { setTags(e, [...(e.tags ?? []), tagDraft.trim()]); tagDraft = ''; } }}>
                            <input class="input small-input" list="deck-tags" placeholder="Add a tag" bind:value={tagDraft} aria-label="Add a tag to {e.name}" />
                          </form>
                        </div>
                        <button role="menuitem" class="danger" onclick={() => edit({ action: 'remove', card: e.name, zone: e.zone }, `Removed ${e.name}.`)}>Remove</button>
                      </div>
                    {/if}
                  </span>
                </span>
              </li>
            {/each}
          </ul>
        {:else}
          <div class="images">
            {#each g.entries as e (key(e))}
              <div
                class="image-cell"
                class:picked={selected.has(key(e))}
                class:dragging={dragging.some((d) => key(d) === key(e))}
                draggable="true"
                role="listitem"
                ondragstart={(ev) => startDrag(ev, e, g.title)}
                ondragend={endDrag}
                onmouseenter={() => (keyTarget = e)}
                onfocusin={() => (keyTarget = e)}
              >
                <button class="image" class:foil={e.foil} onclick={() => e.card && app.open(e.card, { deck: deck.name, entry: e })} aria-label={e.name}>
                  {#if entryImage(e)}
                    <img class="card-img" src={entryImage(e)} alt={e.name} loading="lazy" draggable="false" />
                  {:else}
                    <span class="card-img placeholder">{e.name}</span>
                  {/if}
                  {#if e.quantity > 1}<span class="stack-qty num">×{e.quantity}</span>{/if}
                </button>
                <input type="checkbox" class="pick img-pick" class:on={selected.size > 0} checked={selected.has(key(e))} onchange={() => toggleSelect(e)} aria-label="Select {e.name}" />
                {#if e.tags?.length}
                  <span class="img-tags">
                    {#each e.tags.slice(0, 3) as t (t)}<span class="tag small" class:active={tagFilter === t}>#{t}</span>{/each}
                    {#if e.tags.length > 3}<span class="tag small">+{e.tags.length - 3}</span>{/if}
                  </span>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </section>
    {/each}
  </div>

  <p class="keys small muted">
    Keys on the card under the pointer: <kbd>+</kbd> <kbd>−</kbd> quantity · <kbd>S</kbd> sideboard · <kbd>C</kbd> Considering ·
    <kbd>M</kbd> deck · <kbd>Del</kbd> remove · <kbd>X</kbd> select · <kbd>/</kbd> add a card. Drag cards onto a group to move or retag them.
  </p>

  {#if view === 'list'}
    <aside class="preview">
      {#if previewCard?.image_uri}
        <img class="card-img" src={previewCard.image_uri} alt={previewCard.name} />
      {/if}
    </aside>
  {/if}
</div>

<style>
  .group.drop-ok {
    outline: 2px dashed var(--brass);
    outline-offset: 4px;
    border-radius: var(--r-sm);
  }
  .drop-hint {
    margin-left: 0.5rem;
    color: var(--brass-strong);
    font-weight: 700;
  }
  .entry.dragging,
  .image-cell.dragging {
    opacity: 0.45;
  }
  .image-cell {
    position: relative;
    min-width: 0;
  }
  .image-cell.picked .image {
    outline: 3px solid var(--brass);
    outline-offset: 2px;
    border-radius: var(--r-card);
  }
  .img-pick {
    position: absolute;
    top: 6px;
    left: 6px;
    width: 20px;
    height: 20px;
    z-index: 2;
  }
  .img-tags {
    position: absolute;
    left: 4px;
    right: 4px;
    bottom: 8%;
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    justify-content: center;
    pointer-events: none;
  }
  .img-tags .tag {
    background: rgba(13, 10, 18, 0.82);
    color: #fff;
  }
  .keys {
    grid-column: 1 / -1;
    margin: 0.5rem 0 0;
  }
  .keys kbd {
    font: inherit;
    font-weight: 700;
    padding: 0 0.3rem;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  @media (hover: none) {
    .keys {
      display: none;
    }
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
    margin-bottom: 1.25rem;
  }
  .add {
    position: relative;
    flex: 1 1 280px;
    max-width: 440px;
    display: flex;
    align-items: center;
    color: var(--muted);
  }
  .add > :global(svg) {
    position: absolute;
    left: 0.65rem;
  }
  .add input {
    padding-left: 2.1rem;
  }
  .results {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 25;
    margin: 0;
    padding: 0.3rem;
    list-style: none;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow);
  }
  .results button {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.4rem 0.55rem;
    border: 0;
    background: none;
    border-radius: var(--r-sm);
    cursor: pointer;
    text-align: left;
  }
  .results button.active,
  .results button:hover {
    background: var(--panel-2);
  }
  .r-name {
    flex: 1;
  }

  .group-by {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--muted);
  }
  .group-by select {
    width: auto;
  }
  .filters {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
    margin: -0.5rem 0 1rem;
  }
  .filter {
    max-width: 220px;
  }
  .tag-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }
  .chip {
    padding: 0.2rem 0.6rem;
    border-radius: 99px;
    border: 1px solid var(--line);
    background: var(--panel);
    color: var(--text-2);
    font: inherit;
    font-size: 0.8rem;
    cursor: pointer;
  }
  .chip[aria-pressed='true'] {
    border-color: var(--brass);
    color: var(--brass-strong);
  }
  .bulk {
    position: sticky;
    top: 4rem;
    z-index: 20;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    padding: 0.5rem 0.75rem;
    margin-bottom: 1rem;
    border-color: var(--brass);
  }
  .bulk .danger {
    color: var(--danger);
  }
  .small-input {
    padding: 0.25rem 0.5rem;
    font-size: 0.85rem;
    width: 9rem;
  }
  .tag {
    margin-left: 0.3rem;
    padding: 0 0.35rem;
    border-radius: 99px;
    background: var(--panel-2);
    color: var(--muted);
    font-size: 0.7rem;
  }
  .tag.active {
    color: var(--brass-strong);
  }
  .pick {
    opacity: 0;
    margin: 0;
  }
  .entry:hover .pick,
  .image-cell:hover .pick,
  .pick.on,
  .pick:focus-visible {
    opacity: 1;
  }
  .entry.picked {
    background: color-mix(in srgb, var(--brass) 12%, transparent);
  }
  .menu-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    padding: 0.4rem 0.5rem;
    border-top: 1px solid var(--line-soft);
    border-bottom: 1px solid var(--line-soft);
    margin: 0.2rem 0;
  }
  .menu-tags .removable {
    margin: 0;
    border: 0;
    cursor: pointer;
  }

  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 260px;
    gap: 2rem;
    align-items: start;
  }
  .layout.visual {
    grid-template-columns: minmax(0, 1fr);
  }
  .groups {
    columns: 2 320px;
    column-gap: 2rem;
  }
  .visual .groups {
    columns: auto;
  }
  .group {
    break-inside: avoid;
    margin-bottom: 1.6rem;
  }
  .group h3 {
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
    padding-bottom: 0.35rem;
    margin-bottom: 0.25rem;
    border-bottom: 1px solid var(--line-soft);
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 34px;
    padding: 0 0.3rem;
    border-radius: var(--r-sm);
  }
  .entry:hover {
    background: var(--panel);
  }
  .qty {
    width: 1.4rem;
    text-align: right;
    color: var(--muted);
    font-size: 0.9rem;
  }
  .name {
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name:hover {
    color: var(--brass-strong);
  }
  .name.unknown {
    color: var(--danger);
  }
  .set {
    margin-left: 0.35rem;
    font-size: 0.72rem;
  }
  .foil-tag {
    margin-left: 0.3rem;
    font-size: 0.7rem;
    color: var(--brass-strong);
  }
  .image.foil :global(img) {
    box-shadow: 0 0 0 2px var(--brass);
  }
  .gc {
    color: var(--brass);
    font-size: 0.7rem;
    margin-left: 0.25rem;
  }
  .price {
    width: 4.6rem;
    text-align: right;
  }
  .tools {
    display: flex;
    opacity: 0;
  }
  .entry:hover .tools,
  .entry:focus-within .tools {
    opacity: 1;
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 30;
    display: grid;
    min-width: 170px;
    padding: 0.3rem;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow);
  }
  .menu button {
    padding: 0.45rem 0.6rem;
    border: 0;
    background: none;
    text-align: left;
    border-radius: var(--r-sm);
    cursor: pointer;
  }
  .menu button:hover {
    background: var(--panel-2);
  }
  .menu .danger {
    color: var(--danger);
  }

  .preview {
    position: sticky;
    top: 4.5rem;
  }

  .images {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 0.75rem;
  }
  .image {
    position: relative;
    padding: 0;
    border: 0;
    background: none;
    cursor: zoom-in;
  }
  .placeholder {
    display: grid;
    place-items: center;
    padding: 0.5rem;
    text-align: center;
    color: var(--muted);
  }
  .stack-qty {
    position: absolute;
    right: 6px;
    top: 28%;
    padding: 0.05rem 0.45rem;
    border-radius: 99px;
    background: var(--ink);
    border: 1px solid var(--brass);
    color: var(--brass-strong);
    font-weight: 700;
    font-size: 0.85rem;
  }

  @media (max-width: 1100px) {
    .layout {
      grid-template-columns: minmax(0, 1fr);
    }
    .preview {
      display: none;
    }
  }
  @media (hover: none) {
    .entry {
      min-height: 46px;
    }
    .name {
      align-self: stretch;
    }
    .tools :global(.btn.small.icon) {
      width: 38px;
      min-height: 38px;
    }
    .tools {
      opacity: 1;
    }
    .pick {
      opacity: 1;
    }
    .price {
      display: none;
    }
  }
  @media (max-width: 640px) {
    .images {
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: 0.4rem;
    }
  }
</style>
