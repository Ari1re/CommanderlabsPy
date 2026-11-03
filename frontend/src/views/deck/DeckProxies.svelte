<!--
Proxies tab: MPC proxy orders. Finds art for every card (MPC Autofill's
community scans and official Scryfall art), lets you pick per card, then
exports the order or fills it straight onto MakePlayingCards/PrinterStudio
with the MPC Autofill desktop tool (desktop only).

Things to work on:
- Art choices are lost when the app restarts (kept in memory in the core).
- Once the autofill tool is running in its own window, this tab can't see
  its progress.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api, copyText, download, runJob, runtime } from '../../lib/api';
  import { app } from '../../lib/store.svelte';
  import type { MpcCard, MpcConfig, MpcOrder } from '../../lib/types';
  import Icon from '../../components/Icon.svelte';

  let { name }: { name: string } = $props();

  let order = $state<MpcOrder | null>(null);
  let config = $state<MpcConfig | null>(null);
  let searching = $state<{ percent: number; detail: string } | null>(null);
  let notFound = $state<string[]>([]);
  let focus = $state(0);
  let showBack = $state(false);
  let settingsOpen = $state(false);
  let exported = $state<{
    text: string;
    xml: string | null;
    xml_path: string | null;
    text_path: string;
    error: string | null;
    file_stem: string;
    official_images: number;
    images_dir: string | null;
    download_failed: string[];
  } | null>(null);
  let exporting = $state<{ percent: number; detail: string } | null>(null);

  // MPC Autofill desktop tool: fills the project into the printing site.
  let autofill = $state<{ supported: boolean; installed: boolean; version: string; sites: string[]; browsers: string[] } | null>(null);
  let filling = $state<{ percent: number; step: string; detail: string } | null>(null);
  let filled = $state<{ terminal: string; site: string; browser: string; run_dir: string } | null>(null);
  api<any>('mpc_autofill_status').then((s) => (autofill = s)).catch(() => (autofill = null));

  const SITE_LABEL: Record<string, string> = {
    MakePlayingCards: 'MakePlayingCards',
    PrinterStudio: 'PrinterStudio (US)',
    PrinterStudioUK: 'PrinterStudio (UK)',
    PrinterStudioDE: 'PrinterStudio (DE)',
    PrinterStudioES: 'PrinterStudio (ES)',
    PrinterStudioFR: 'PrinterStudio (FR)',
  };

  async function fillOrder() {
    filled = null;
    filling = { percent: 0, step: 'Starting', detail: '' };
    try {
      filled = await runJob('mpc_autofill', { name }, (j) => (filling = { percent: j.percent, step: j.step, detail: j.detail }), 600);
      if (autofill) autofill = { ...autofill, installed: true };
    } catch (err) {
      app.error(err);
    } finally {
      filling = null;
    }
  }
  let artFilter = $state<'all' | 'official' | 'community'>('all');
  let loadingMore = $state(false);

  const STOCKS = ['(S30) Standard Smooth', '(S33) Superior Smooth', '(M31) Linen', '(P10) Plastic', '(S27) Smooth'];

  onMount(async () => {
    try {
      [order, config] = await Promise.all([api<MpcOrder>('mpc_order', { name, rebuild: true }), api<MpcConfig>('mpc_config')]);
    } catch (err) {
      app.error(err);
    }
  });

  async function search() {
    searching = { percent: 0, detail: '' };
    try {
      const res = await runJob<MpcOrder>('mpc_search', { name }, (j) => (searching = { percent: j.percent, detail: j.detail }), 700);
      order = res;
      notFound = res.not_found ?? [];
      if (res.warning) app.toast(res.warning, 'error', 8000);
      else app.toast(`Found art for ${res.resolved} of ${res.faces} faces.`, 'success');
    } catch (err) {
      app.error(err);
    } finally {
      searching = null;
    }
  }

  async function select(index: number, back: boolean, selected: number) {
    try {
      order = await api<MpcOrder>('mpc_select', { name, index, back, selected });
    } catch (err) {
      app.error(err);
    }
  }

  async function setConfig(patch: Partial<MpcConfig>) {
    try {
      config = await api<MpcConfig>('mpc_config_set', { config: patch });
      if ('include_sideboard' in patch) order = await api<MpcOrder>('mpc_order', { name, rebuild: true });
      else if (order) order = { ...order, stock: config.stock, foil: config.foil };
    } catch (err) {
      app.error(err);
    }
  }

  async function exportOrder() {
    exporting = { percent: 0, detail: '' };
    try {
      exported = await runJob('mpc_export', { name }, (j) => (exporting = { percent: j.percent, detail: j.detail }), 600);
      if (exported?.error) app.toast(exported.error, 'error', 7000);
      else app.toast('MPC order saved.', 'success');
    } catch (err) {
      app.error(err);
    } finally {
      exporting = null;
    }
  }

  async function loadMore() {
    loadingMore = true;
    try {
      order = await api<MpcOrder>('mpc_more', { name, index: focus, back: showBack });
    } catch (err) {
      app.error(err);
    } finally {
      loadingMore = false;
    }
  }

  function faceOf(card: MpcCard, back: boolean) {
    return back && card.back ? card.back : card.front;
  }

  let current = $derived(order?.cards[focus]);
  let face = $derived(current ? faceOf(current, showBack) : null);
  let officialCount = $derived(face?.candidates.filter((c) => c.official).length ?? 0);
  let communityCount = $derived((face?.candidates.length ?? 0) - officialCount);
  /** Candidates with their index in the face, filtered by source. */
  let shownCands = $derived(
    (face?.candidates ?? [])
      .map((cand, j) => ({ cand, j }))
      .filter(({ cand }) => artFilter === 'all' || (artFilter === 'official') === !!cand.official),
  );
</script>

{#if !order}
  <p class="muted">Preparing your proxy order…</p>
{:else}
  <div class="head">
    <div>
      <h2>Print proxies with MPC Autofill</h2>
      <p class="dim small">
        {order.total_slots} cards round up to an MPC order of {order.bracket}. Art found for {order.resolved} of {order.faces} faces.
      </p>
    </div>
    <div class="row">
      <button class="btn ghost" onclick={() => (settingsOpen = !settingsOpen)}><Icon name="settings" size={16} /> Order options</button>
      <button class="btn" onclick={search} disabled={!!searching}><Icon name="search" size={16} /> {order.resolved ? 'Search again' : 'Find proxy art'}</button>
      <button class="btn" onclick={exportOrder} disabled={!!exporting}><Icon name="download" size={16} /> {exporting ? 'Exporting…' : 'Export order'}</button>
      {#if autofill?.supported}
        <button class="btn primary" onclick={fillOrder} disabled={!!filling || !order.resolved}>
          <Icon name="external" size={16} /> {filling ? 'Starting…' : `Fill on ${SITE_LABEL[config?.autofill_site ?? 'MakePlayingCards'] ?? 'MakePlayingCards'}`}
        </button>
      {/if}
    </div>
  </div>

  {#if autofill?.supported}
    <section class="panel autofill">
      <div class="row">
        <div class="grow">
          <b>Order on the printer's site</b>
          <p class="small dim">
            Runs the MPC Autofill desktop tool ({autofill.version}{autofill.installed ? '' : ', downloaded from GitHub the first time, about 50 MB'}).
            It opens your browser, creates the project, uploads every card's art and saves it to your account. Then you
            review it there and add it to your cart. It needs Chrome, Edge or Brave installed, and asks one question in its
            window: start a new project, or add to one you saved.
          </p>
        </div>
        {#if config}
          <label class="field"><span class="small">Site</span>
            <select class="input" value={config.autofill_site} onchange={(e) => setConfig({ autofill_site: (e.target as HTMLSelectElement).value })}>
              {#each autofill.sites as s (s)}<option value={s}>{SITE_LABEL[s] ?? s}</option>{/each}
            </select>
          </label>
          <label class="field"><span class="small">Browser</span>
            <select class="input" value={config.autofill_browser} onchange={(e) => setConfig({ autofill_browser: (e.target as HTMLSelectElement).value })}>
              {#each autofill.browsers as b (b)}<option value={b}>{b[0].toUpperCase() + b.slice(1)}</option>{/each}
            </select>
          </label>
        {/if}
      </div>
      {#if filling}
        <div class="progress"><i style:width="{filling.percent}%"></i></div>
        <span class="small dim">{filling.step} {filling.detail}</span>
      {:else if filled}
        <p class="small ok">
          MPC Autofill is running in {filled.terminal}. Follow it there: pick “Create a new project”, then let it upload.
          When it finishes, check the project on {SITE_LABEL[filled.site] ?? filled.site} and add it to your cart.
        </p>
      {:else if !order.resolved}
        <p class="small dim">Find proxy art first, so every card has an image to upload.</p>
      {/if}
    </section>
  {/if}

  {#if searching}
    <div class="searching">
      <div class="progress"><i style:width="{searching.percent}%"></i></div>
      <span class="small dim">Searching MPC Autofill and Scryfall… {searching.detail}</span>
    </div>
  {/if}
  {#if exporting}
    <div class="searching">
      <div class="progress"><i style:width="{exporting.percent}%"></i></div>
      <span class="small dim">Saving the order… {exporting.detail}</span>
    </div>
  {/if}

  {#if settingsOpen && config}
    <section class="panel options">
      <label class="field"><span>Card stock</span>
        <select class="input" value={config.stock} onchange={(e) => setConfig({ stock: (e.target as HTMLSelectElement).value })}>
          {#each STOCKS as s (s)}<option>{s}</option>{/each}
        </select>
      </label>
      <label class="check"><input type="checkbox" checked={config.foil} onchange={(e) => setConfig({ foil: (e.target as HTMLInputElement).checked })} /> Foil</label>
      <label class="check"><input type="checkbox" checked={config.include_sideboard} onchange={(e) => setConfig({ include_sideboard: (e.target as HTMLInputElement).checked })} /> Include sideboard</label>
      <label class="check"><input type="checkbox" checked={config.include_official} onchange={(e) => setConfig({ include_official: (e.target as HTMLInputElement).checked })} /> Include official card art (Scryfall)</label>
      <label class="check"><input type="checkbox" checked={config.prefer_official} onchange={(e) => setConfig({ prefer_official: (e.target as HTMLInputElement).checked })} /> Official art first</label>
      <label class="field"><span>Art choices per card <span class="muted">(0 = all)</span></span>
        <input class="input" type="number" min="0" step="10" value={config.max_candidates} onchange={(e) => setConfig({ max_candidates: Number((e.target as HTMLInputElement).value) })} />
      </label>
      <label class="field"><span>Minimum DPI</span>
        <input class="input" type="number" min="0" step="50" value={config.minimum_dpi} onchange={(e) => setConfig({ minimum_dpi: Number((e.target as HTMLInputElement).value) })} />
      </label>
      <label class="field"><span>Preferred sources <span class="muted">(comma separated)</span></span>
        <input class="input" value={config.preferred_sources.join(', ')} onchange={(e) => setConfig({ preferred_sources: (e.target as HTMLInputElement).value.split(',').map((s) => s.trim()).filter(Boolean) })} />
      </label>
      <label class="field"><span>Cardback image ID <span class="muted">(blank = default)</span></span>
        <input class="input" value={config.cardback_id} onchange={(e) => setConfig({ cardback_id: (e.target as HTMLInputElement).value.trim() })} />
      </label>
      <label class="field wide"><span>MPC Autofill server</span>
        <input class="input" value={config.backend_url} onchange={(e) => setConfig({ backend_url: (e.target as HTMLInputElement).value.trim() })} />
      </label>
    </section>
  {/if}

  <div class="layout">
    <ul class="list">
      {#each order.cards as c, i (c.name + i)}
        {@const missing = !c.front.candidates.length || (c.back && !c.back.candidates.length)}
        <li>
          <button class="item" class:on={i === focus} class:missing onclick={() => { focus = i; showBack = false; }}>
            <span class="num qty">{c.quantity}</span>
            <span class="nm">{c.name}{c.back ? ' ⟲' : ''}</span>
            <span class="small muted">{c.front.candidates.length ? `${c.front.selected + 1}/${c.front.candidates.length}${c.front.pending_ids?.length ? '+' : ''}` : c.front.pending_ids?.length ? 'tap to load' : 'no art'}</span>
          </button>
        </li>
      {/each}
    </ul>

    <section class="art">
      {#if current && face}
        <div class="row">
          <h3>{current.name}</h3>
          {#if current.back}
            <div class="segmented">
              <button aria-pressed={!showBack} onclick={() => (showBack = false)}>Front</button>
              <button aria-pressed={showBack} onclick={() => (showBack = true)}>Back</button>
            </div>
          {/if}
        </div>
        {#if face.candidates.length}
          {@const img = face.candidates[face.selected]}
          <div class="chosen">
            <img class="card-img" src={img.thumbnail_url} alt={img.name} />
            <div class="small dim">
              {#if img.official}
                <span class="badge brass">Official</span> {img.printing}
              {:else}
                {img.source_name || 'Unknown source'} · {img.dpi} DPI{img.tags?.length ? ` · ${img.tags.join(', ')}` : ''}
              {/if}
            </div>
          </div>
          <div class="cand-head">
            <h3 class="small dim">All art ({face.candidates.length}{face.pending_ids?.length ? ` of ${face.candidates.length + face.pending_ids.length}` : ''})</h3>
            <div class="segmented" role="group" aria-label="Art source">
              <button aria-pressed={artFilter === 'all'} onclick={() => (artFilter = 'all')}>All</button>
              <button aria-pressed={artFilter === 'official'} onclick={() => (artFilter = 'official')}>Official {officialCount}</button>
              <button aria-pressed={artFilter === 'community'} onclick={() => (artFilter = 'community')}>MPC {communityCount}{face.pending_ids?.length ? '+' : ''}</button>
            </div>
          </div>
          <div class="cands">
            {#each shownCands as { cand, j } (cand.identifier)}
              <button class="cand" class:on={j === face.selected} onclick={() => select(focus, showBack, j)} title={cand.official ? `Official · ${cand.printing}` : `${cand.source_name} · ${cand.dpi} DPI`}>
                <img class="card-img" src={cand.thumbnail_url} alt="" loading="lazy" />
                {#if cand.official}<span class="cand-tag">Official</span>{/if}
              </button>
            {/each}
          </div>
          {#if face.pending_ids?.length && artFilter !== 'official'}
            <button class="btn more" onclick={loadMore} disabled={loadingMore}>
              {loadingMore ? 'Loading…' : `Load ${face.pending_ids.length} more MPC images`}
            </button>
          {/if}
        {:else if face.pending_ids?.length}
          <button class="btn" onclick={loadMore} disabled={loadingMore}>{loadingMore ? 'Loading…' : `Load ${face.pending_ids.length} MPC images`}</button>
        {:else}
          <p class="muted">No proxy art yet. Click “Find proxy art”.</p>
        {/if}
      {/if}
    </section>
  </div>

  {#if notFound.length}
    <p class="small dim">No MPC art found for: {notFound.join(', ')}.</p>
  {/if}

  {#if exported}
    <section class="panel exported">
      <h3>Your order</h3>
      {#if exported.xml_path}
        <p class="small">Saved <b>{exported.xml_path}</b>. Open it with the MPC Autofill desktop tool, or import it on mpcfill.com.</p>
      {/if}
      {#if exported.download_failed?.length}
        <p class="small error">
          Couldn't download {exported.download_failed.length} official scan{exported.download_failed.length === 1 ? '' : 's'}
          ({exported.download_failed.slice(0, 5).join(', ')}{exported.download_failed.length > 5 ? '…' : ''}). Check your connection
          and export again; those cards still point at Scryfall's web address.
        </p>
      {/if}
      {#if exported.official_images}
        <p class="small">
          {exported.official_images} official scan{exported.official_images === 1 ? '' : 's'} saved in <b>{exported.images_dir}</b>.
          Official art isn't on Google Drive, so the order points at these files: use the MPC Autofill desktop tool (v4 or later)
          on this computer, or move the folder with the XML and keep the paths the same. The mpcfill.com website can't read local files.
        </p>
      {/if}
      <p class="small">The plain list is in <b>{exported.text_path}</b>. Paste it into mpcfill.com's “Add cards → Text” box.</p>
      <div class="row">
        {#if !runtime.isTauri && exported.xml}
          <button class="btn" onclick={() => download(`${exported!.file_stem}.mpc.xml`, exported!.xml!, 'application/xml')}><Icon name="download" size={15} /> Download XML</button>
        {/if}
        <button class="btn" onclick={async () => { await copyText(exported!.text); app.toast('List copied.', 'success'); }}><Icon name="copy" size={15} /> Copy list</button>
        <a class="btn ghost" href="https://mpcfill.com" target="_blank" rel="noreferrer">Open mpcfill.com <Icon name="external" size={14} /></a>
      </div>
    </section>
  {/if}
{/if}

<style>
  .head {
    display: flex;
    justify-content: space-between;
    align-items: end;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 1rem;
  }
  .searching {
    display: grid;
    gap: 0.35rem;
    margin-bottom: 1rem;
    max-width: 30rem;
  }
  .options {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 0.9rem 1.25rem;
    padding: 1rem 1.25rem;
    margin-bottom: 1.25rem;
    align-items: end;
  }
  .options .wide {
    grid-column: 1 / -1;
  }
  .check {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    min-height: 40px;
  }
  .check input {
    accent-color: var(--brass);
    width: 18px;
    height: 18px;
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(240px, 340px) minmax(0, 1fr);
    gap: 1.5rem;
    align-items: start;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 70vh;
    overflow: auto;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--panel);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.45rem 0.75rem;
    border: 0;
    background: none;
    cursor: pointer;
    text-align: left;
    border-bottom: 1px solid var(--line-soft);
  }
  .item:hover {
    background: var(--panel-2);
  }
  .item.on {
    background: color-mix(in srgb, var(--brass) 14%, var(--panel));
  }
  .item.missing .nm {
    color: var(--muted);
  }
  .qty {
    width: 1.5rem;
    text-align: right;
    color: var(--muted);
  }
  .nm {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .art {
    position: sticky;
    top: 4.5rem;
    display: grid;
    gap: 0.75rem;
  }
  .chosen {
    display: grid;
    gap: 0.4rem;
    width: min(280px, 100%);
  }
  .cands {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(92px, 1fr));
    gap: 0.5rem;
  }
  .cand {
    padding: 2px;
    border: 2px solid transparent;
    border-radius: 8px;
    background: none;
    cursor: pointer;
  }
  .cand.on {
    border-color: var(--brass);
  }
  .autofill {
    display: grid;
    gap: 0.5rem;
    padding: 0.9rem 1rem;
    margin-bottom: 1rem;
  }
  .autofill .grow {
    flex: 1 1 22rem;
  }
  .autofill .ok {
    color: var(--ok);
  }
  .exported {
    margin-top: 1.5rem;
    padding: 1rem 1.25rem;
    display: grid;
    gap: 0.5rem;
  }
  @media (max-width: 760px) {
    .layout {
      grid-template-columns: minmax(0, 1fr);
    }
    .list {
      max-height: 40vh;
    }
    .art {
      position: static;
    }
  }
  .cand {
    position: relative;
  }
  .cand-tag {
    position: absolute;
    left: 4px;
    bottom: 4px;
    padding: 0 0.35rem;
    border-radius: 99px;
    background: rgba(14, 10, 20, 0.85);
    border: 1px solid var(--brass);
    color: #fff;
    font-size: 0.62rem;
    font-weight: 700;
  }
  .cand-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    margin: 0.75rem 0 0.4rem;
  }
  .cand-head h3 {
    margin: 0;
  }
  .more {
    margin-top: 0.75rem;
  }
  .error {
    color: var(--danger);
  }
</style>
