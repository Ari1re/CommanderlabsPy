<script lang="ts">
  import { onMount } from 'svelte';
  import { api, runJob } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Icon from '../components/Icon.svelte';

  let checking = $state(false);

  let { onUpdateCards }: { onUpdateCards: () => void } = $props();

  let ai = $state<any>(null);
  let apiKey = $state('');
  let starting = $state(false);

  onMount(async () => {
    await app.refreshStatus();
    // Opening Settings always asks GitHub, so a new build shows up right away.
    checking = true;
    app.checkUpdate(true, false).finally(() => (checking = false));
    try {
      ai = await api('ai_status');
    } catch (err) {
      app.error(err);
    }
  });

  async function setTheme(theme: typeof app.theme) {
    app.theme = theme;
    app.applyTheme();
    await app.saveSetting('theme', theme);
  }

  async function setGeneration(value: typeof app.generation) {
    app.generation = value;
    await app.saveSetting('generation', value);
    app.toast(value === 'deep' ? 'New decks use deep learning generation.' : 'New decks use peer-based generation.', 'success');
  }

  async function setCurrency(currency: typeof app.currency) {
    app.currency = currency;
    await app.saveSetting('currency', currency);
  }

  async function setAi(patch: Record<string, unknown>) {
    try {
      ai = await api('ai_config_set', patch);
      app.toast('AI settings saved.', 'success');
    } catch (err) {
      app.error(err);
    }
  }

  async function startAi() {
    starting = true;
    try {
      await runJob('ai_start', {}, undefined, 1500);
      ai = await api('ai_status');
    } catch (err) {
      app.error(err);
    } finally {
      starting = false;
    }
  }

  async function stopAi() {
    ai = await api('ai_stop');
  }

  // Built-in model downloads: model id → progress text.
  let downloading = $state<Record<string, string>>({});

  async function downloadModel(model: string) {
    downloading[model] = 'Starting…';
    try {
      ai = await runJob('ai_model_download', { model }, (job) => {
        downloading[model] = job.detail || `${job.percent}%`;
      }, 1000);
      app.toast('Model downloaded. AI is ready to start.', 'success');
    } catch (err) {
      app.error(err);
    } finally {
      delete downloading[model];
    }
  }

  // Online services with OpenAI-compatible APIs.
  const ONLINE = [
    { id: 'openai', name: 'OpenAI', base: 'https://api.openai.com/v1', model: 'gpt-4.1-mini', keys: 'https://platform.openai.com/api-keys' },
    { id: 'anthropic', name: 'Anthropic (Claude)', base: 'https://api.anthropic.com/v1', model: 'claude-haiku-5-5', keys: 'https://console.anthropic.com/settings/keys' },
    { id: 'gemini', name: 'Google Gemini', base: 'https://generativelanguage.googleapis.com/v1beta/openai', model: 'gemini-2.5-flash', keys: 'https://aistudio.google.com/apikey' },
    { id: 'openrouter', name: 'OpenRouter', base: 'https://openrouter.ai/api/v1', model: 'openrouter/auto', keys: 'https://openrouter.ai/keys' },
    { id: 'groq', name: 'Groq', base: 'https://api.groq.com/openai/v1', model: 'llama-3.3-70b-versatile', keys: 'https://console.groq.com/keys' },
    { id: 'mistral', name: 'Mistral', base: 'https://api.mistral.ai/v1', model: 'mistral-small-latest', keys: 'https://console.mistral.ai/api-keys' },
    { id: 'custom', name: 'Other', base: '', model: '', keys: '' },
  ];

  let onlineId = $state<string | null>(null);
  let apiBase = $state('');
  let apiModel = $state('');
  let apiModels = $state<string[]>([]);
  let listing = $state(false);
  let testing = $state(false);
  let testResult = $state<{ ok: boolean; text: string } | null>(null);

  // Open the online form on the service already configured.
  $effect(() => {
    if (ai && onlineId === null && ai.config.provider === 'api') {
      const base = (ai.config.api_base_url ?? '').replace(/\/+$/, '');
      onlineId = ONLINE.find((o) => o.base && base.startsWith(o.base.replace(/\/v1$/, '')))?.id ?? 'custom';
      apiBase = ai.config.api_base_url;
      apiModel = ai.config.api_model;
    }
  });

  let usingLabel = $derived.by(() => {
    if (!ai) return '';
    if (ai.config.provider === 'api') return `Online: ${ai.config.api_model || 'no model set'}`;
    if (ai.config.provider === 'ollama') return `Ollama: ${ai.config.model}`;
    if (ai.uses_built_in) {
      const m = ai.models?.find((x: any) => x.model === ai.config.model && x.installed) ?? ai.models?.find((x: any) => x.installed);
      return m ? `On this device: ${m.name}` : 'Nothing yet. Download a model or connect an online AI.';
    }
    return `llama.cpp server: ${ai.config.base_url}`;
  });

  function pickOnline(o: (typeof ONLINE)[number]) {
    onlineId = o.id;
    apiBase = o.base || apiBase;
    apiModel = o.model || apiModel;
    apiModels = [];
  }

  async function useOnline() {
    if (!apiBase.trim() || !apiModel.trim()) {
      app.toast('Fill in the API address and model first.');
      return;
    }
    const patch: Record<string, unknown> = { provider: 'api', api_base_url: apiBase.trim(), api_model: apiModel.trim() };
    if (apiKey.trim()) patch.api_key = apiKey.trim();
    await setAi(patch);
    apiKey = '';
  }

  async function loadApiModels() {
    listing = true;
    try {
      const res = await api<{ models: string[] }>('ai_api_models', { api_base_url: apiBase.trim(), api_key: apiKey.trim() });
      apiModels = res.models;
      app.toast(res.models.length ? `${res.models.length} models found. Pick one in the Model box.` : 'The service listed no models; type one in.');
    } catch (err) {
      app.error(err);
    } finally {
      listing = false;
    }
  }

  async function testAi() {
    testing = true;
    testResult = null;
    try {
      const r = await api<any>('ai_test');
      testResult = { ok: true, text: `Working: ${r.provider} ${r.model} answered "${r.reply}" in ${r.seconds}s.` };
      ai = await api('ai_status');
    } catch (err) {
      testResult = { ok: false, text: `Not working: ${(err as Error).message}` };
    } finally {
      testing = false;
    }
  }

  async function deleteModel(model: string, name: string) {
    if (!confirm(`Delete the ${name} model? You can download it again later.`)) return;
    try {
      ai = await api('ai_model_delete', { model });
    } catch (err) {
      app.error(err);
    }
  }
</script>

<h1 class="title">Settings</h1>

<div class="sections">
  <section>
    <h2>Appearance</h2>
    <div class="setting">
      <span>Theme</span>
      <div class="segmented" role="group" aria-label="Theme">
        <button aria-pressed={app.theme === 'dark'} onclick={() => setTheme('dark')}>Dark</button>
        <button aria-pressed={app.theme === 'light'} onclick={() => setTheme('light')}>Light</button>
        <button aria-pressed={app.theme === 'system'} onclick={() => setTheme('system')}>Match system</button>
      </div>
    </div>
    <div class="setting">
      <span>Prices</span>
      <div class="segmented" role="group" aria-label="Currency">
        <button aria-pressed={app.currency === 'usd'} onclick={() => setCurrency('usd')}>US dollars</button>
        <button aria-pressed={app.currency === 'eur'} onclick={() => setCurrency('eur')}>Euros</button>
        <button aria-pressed={app.currency === 'tix'} onclick={() => setCurrency('tix')}>MTGO tix</button>
      </div>
    </div>
  </section>

  <section>
    <h2>Updates {#if app.update?.available}<span class="badge brass">New</span>{/if}</h2>
    <p class="dim">
      {#if app.update?.available}
        Commander Lab {app.update.latest_version} is available. You have {app.update.current_version}{app.update.current_build ? ` (build ${app.update.current_build})` : ''}.
      {:else if app.update}
        {app.update.install === 'none' ? app.update.how : `You have the latest version (${app.update.current_version}${app.update.current_build ? `, build ${app.update.current_build}` : ''}).`}
      {:else if app.updateError}
        Couldn't check GitHub for updates: {app.updateError}
      {:else}
        Commander Lab {app.status?.version}{app.status?.build ? ` (build ${app.status.build})` : ''}. It checks GitHub for new builds when it starts.
      {/if}
    </p>
    <div class="row">
      <button class="btn" disabled={checking} onclick={async () => { checking = true; const u = await app.checkUpdate(true); checking = false; if (u && !u.available) app.toast(u.install === 'none' ? 'Development build: no automatic updates.' : 'You have the latest version.', 'success'); }}>
        <Icon name="refresh" size={16} /> {checking ? 'Checking…' : 'Check for updates'}
      </button>
      <a class="btn ghost" href="https://github.com/Ari1re/CommanderlabsPy/releases/latest" target="_blank" rel="noreferrer">
        Releases <Icon name="external" size={14} />
      </a>
      {#if app.update?.available}
        <button class="btn primary" onclick={() => (app.updateOpen = true)}><Icon name="download" size={16} /> Update now</button>
      {/if}
    </div>
  </section>

  <section>
    <h2>Deck generation</h2>
    <div class="setting">
      <div>
        <span>Generator</span>
        <p class="small muted">
          {#if app.generation === 'deep'}
            Reads the commander's own rules text, gathers EDHREC, decklists, Commander Spellbook and a full rules-text
            search, sorts every card into commonly played, synergy, staples and could use, and builds by role with a power
            floor for the bracket. Each deck gets a report of its sources and why each card is in it. Works on its own;
            turn on AI help when building for even better picks.
          {:else}
            The original generator: builds mostly from what EDHREC and other players' decklists run for the commander.
          {/if}
        </p>
      </div>
      <select class="input gen" value={app.generation} onchange={(e) => setGeneration((e.target as HTMLSelectElement).value as typeof app.generation)} aria-label="Deck generator">
        <option value="deep">Deep learning generation (recommended)</option>
        <option value="peer">Peer-based generation</option>
      </select>
    </div>
  </section>

  <section>
    <h2>Card data</h2>
    <div class="setting">
      <div>
        <span>Scryfall card database</span>
        <p class="small muted">
          {app.status?.card_data_ready ? `${app.status.cards.toLocaleString()} cards. Update for new sets and today's prices.` : 'Not downloaded yet.'}
        </p>
      </div>
      {#if app.updatingCards}
        <div class="upd">
          <div class="progress"><i style:width="{app.updatingCards.percent}%"></i></div>
          <span class="small dim">{app.updatingCards.step}</span>
        </div>
      {:else}
        <button class="btn" onclick={onUpdateCards}><Icon name="refresh" size={16} /> Update card data</button>
      {/if}
    </div>
  </section>

  {#if ai}
    <section>
      <h2>AI model</h2>
      <p class="small dim">
        Optional. Powers "Ask about this deck", the agent's written plans, and AI help when building decks. Without one,
        Commander Lab answers from its own analysis.
      </p>

      <div class="setting current panel">
        <div>
          <span>In use: <b>{usingLabel}</b></span>
          <p class="small muted">{ai.running ? ai.status : 'Not started. It also starts by itself the first time something asks it.'}</p>
        </div>
        {#if ai.running}
          <button class="btn" onclick={stopAi}><Icon name="stop" size={15} /> Stop</button>
        {:else}
          <button class="btn" onclick={startAi} disabled={starting}><Icon name="play" size={15} /> {starting ? 'Starting…' : 'Start'}</button>
        {/if}
        <button class="btn ghost" onclick={testAi} disabled={testing}>{testing ? 'Testing…' : 'Test'}</button>
      </div>
      {#if testResult}<p class="small {testResult.ok ? 'ok' : 'err'}">{testResult.text}</p>{/if}

      {#if ai.built_in}
        <h3>On this device</h3>
        <p class="small dim">
          Runs offline. It only works while answering, so it doesn't drain the battery in the background.
          {#if ai.disk_mb}Models use {(ai.disk_mb / 1000).toFixed(1)} GB.{/if}
        </p>
        {#each ai.models as m (m.model)}
          <div class="setting">
            <div>
              <span>
                {m.name}
                {#if ai.uses_built_in && ai.config.model === m.model && m.installed}<span class="badge ok">In use</span>{/if}
                {#if ai.loaded === m.model}<span class="badge">Loaded</span>{/if}
              </span>
              <p class="small muted">{m.description} · {(m.size_mb / 1000).toFixed(1)} GB</p>
            </div>
            {#if downloading[m.model]}
              <span class="small dim">{downloading[m.model]}</span>
            {:else if !m.installed}
              <button class="btn" onclick={() => downloadModel(m.model)}><Icon name="download" size={15} /> Download</button>
            {:else}
              <div class="row">
                {#if !(ai.uses_built_in && ai.config.model === m.model)}
                  <button class="btn" onclick={() => setAi({ provider: 'llama_cpp', model: m.model })}>Use</button>
                {/if}
                <button class="btn ghost" onclick={() => deleteModel(m.model, m.name)}>Delete</button>
              </div>
            {/if}
          </div>
        {/each}
      {/if}

      <h3>Online AI</h3>
      <p class="small dim">Any service with an OpenAI-compatible API. Pick one, paste your key, and test it. Your key stays on this device.</p>
      <div class="presets">
        {#each ONLINE as o (o.id)}
          <button class="chip" class:chosen={onlineId === o.id} onclick={() => pickOnline(o)}>{o.name}</button>
        {/each}
      </div>
      {#if onlineId}
        {@const o = ONLINE.find((x) => x.id === onlineId)}
        <div class="setting">
          <label class="field grow"><span>API address</span><input class="input" bind:value={apiBase} placeholder="https://…/v1" /></label>
          <label class="field grow"><span>Model</span>
            <input class="input" bind:value={apiModel} list="api-models" placeholder={o?.model ?? 'model name'} />
            <datalist id="api-models">{#each apiModels as name (name)}<option value={name}></option>{/each}</datalist>
          </label>
        </div>
        <div class="setting">
          <label class="field grow"><span>API key {ai.config.has_api_key ? '(one is saved)' : ''}</span>
            <input class="input" type="password" bind:value={apiKey} placeholder={ai.config.has_api_key ? 'Leave empty to keep the saved key' : 'Paste your key'} />
          </label>
          {#if o?.keys}<a class="small" href={o.keys} target="_blank" rel="noreferrer">Get a key <Icon name="external" size={12} /></a>{/if}
        </div>
        <div class="row">
          <button class="btn primary" onclick={useOnline}>{ai.config.provider === 'api' ? 'Save' : 'Use this online AI'}</button>
          <button class="btn ghost" onclick={loadApiModels} disabled={listing}>{listing ? 'Listing…' : 'List models'}</button>
        </div>
      {/if}

      {#if app.status?.platform !== 'android'}
        <details class="advanced">
          <summary class="small dim">Advanced: a local server (Ollama or your own llama.cpp)</summary>
          <div class="setting">
            <span>Ollama</span>
            {#if ai.config.provider === 'ollama'}
              <select class="input model" value={ai.config.model} onchange={(e) => setAi({ model: (e.target as HTMLSelectElement).value })}>
                {#each ai.presets.ollama as p (p.model)}<option value={p.model}>{p.name} — {p.description}</option>{/each}
                {#if !ai.presets.ollama.some((p: any) => p.model === ai.config.model)}<option value={ai.config.model}>{ai.config.model}</option>{/if}
              </select>
            {:else}
              <button class="btn" onclick={() => setAi({ provider: 'ollama' })}>Use Ollama</button>
            {/if}
          </div>
        </details>
      {/if}

      <div class="setting">
        <span>Research the web while optimizing</span>
        <input type="checkbox" class="switch" checked={ai.config.web_research} onchange={(e) => setAi({ web_research: (e.target as HTMLInputElement).checked })} />
      </div>
    </section>
  {/if}

  <section>
    <h2>About</h2>
    <p class="small dim">
      Commander Lab {app.status?.version}. Your decks, card data and exports are stored in
      <b>{app.status?.data_dir}</b>.
    </p>
  </section>
</div>

<style>
  .title {
    margin-bottom: 1.5rem;
  }
  .sections {
    display: grid;
    gap: 2.25rem;
    max-width: 52rem;
  }
  section {
    display: grid;
    gap: 0.75rem;
  }
  .current {
    padding: 0.75rem 1rem;
  }
  .gen {
    max-width: 22rem;
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .chip {
    padding: 0.35rem 0.75rem;
    border-radius: 99px;
    border: 1px solid var(--line);
    background: var(--panel);
    color: var(--text-2);
    font: inherit;
    font-size: 0.85rem;
    font-weight: 700;
    cursor: pointer;
  }
  .chip.chosen {
    border-color: var(--brass);
    color: var(--brass-strong);
  }
  .advanced {
    margin-top: 0.75rem;
  }
  .ok {
    color: var(--ok);
  }
  .err {
    color: var(--danger);
  }
  .setting {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    padding: 0.75rem 0;
    border-bottom: 1px solid var(--line-soft);
  }
  .grow {
    flex: 1 1 240px;
  }
  .model {
    width: auto;
    max-width: 100%;
  }
  .upd {
    display: grid;
    gap: 0.3rem;
    min-width: 220px;
  }
  .switch {
    accent-color: var(--brass);
    width: 20px;
    height: 20px;
  }
</style>
