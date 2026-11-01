<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, runJob, type Job } from '../../lib/api';
  import { app } from '../../lib/store.svelte';
  import { BRACKETS, POWER_LEVELS, type FormatId } from '../../lib/util';
  import Icon from '../../components/Icon.svelte';

  let {
    name,
    bracket,
    format = 'commander',
    onsaved,
  }: { name: string; bracket: number; format?: FormatId; onsaved: (name: string) => void } = $props();

  /** Commander decks are tuned to a bracket; Pauper Commander and Highlander use power levels. */
  let levels = $derived(format === 'commander' ? BRACKETS : POWER_LEVELS[format]);
  let levelWord = $derived(format === 'commander' ? 'bracket' : 'power level');
  /** A Highlander rebuild the agent saved this visit. */
  let rebuilt = $state<string | null>(null);

  interface ChatMessage {
    role: 'user' | 'assistant';
    content: string;
  }

  let ai = $state<any>(null);
  let chat = $state<ChatMessage[]>([]);
  let agent = $state<any>(null);
  let brief = $state('');
  let target = $state(3);
  let intensity = $state<'quick' | 'standard' | 'thorough' | 'exhaustive'>('standard');
  let job = $state<Job | null>(null);
  let starting = $state(false);
  let message = $state('');
  let thinking = $state(false);
  /** The chat job while it works: its steps and the answer so far. */
  let live = $state<Job | null>(null);
  /** A change request waiting for the user to pick a bracket. */
  let pending = $state<string | null>(null);
  /** Agent-side messages from this visit (bracket questions, run summaries). */
  let notes = $state<{ after: number; content: string }[]>([]);
  let chatBox: HTMLDivElement | undefined = $state();

  onMount(async () => {
    target = bracket;
    try {
      const [status, state] = await Promise.all([api('ai_status'), api('agent_state', { name })]);
      ai = status;
      chat = state.chat ?? [];
      agent = state.agent;
      brief = state.brief ?? '';
      if (agent?.target_bracket) target = agent.target_bracket;
    } catch (err) {
      app.error(err);
    }
    scrollChat();
  });

  async function scrollChat() {
    await tick();
    chatBox?.scrollTo({ top: chatBox.scrollHeight });
  }

  async function startAi() {
    starting = true;
    try {
      await runJob('ai_start', {}, undefined, 1500);
      ai = await api('ai_status');
      app.toast('AI model is running.', 'success');
    } catch (err) {
      app.error(err);
    } finally {
      starting = false;
    }
  }

  async function run() {
    job = { id: 0, kind: 'agent', subject: name, status: 'running', percent: 0, step: 'Starting', detail: '', log: [], result: null, error: null };
    scrollChat();
    try {
      agent = await runJob('agent_run', { name, bracket: target, intensity, brief }, (j) => (job = j), 1000);
      note(`Done. I tested ${agent?.rounds?.length ?? 0} versions at Bracket ${target}; the best ones are below. Your deck isn't changed until you save one.`);
    } catch (err) {
      app.error(err);
    } finally {
      job = null;
      scrollChat();
    }
  }

  function note(content: string) {
    notes = [...notes, { after: chat.length, content }];
    scrollChat();
  }

  /** Does the message ask to change the deck (rather than ask about it)? */
  function wantsChanges(text: string): boolean {
    const t = text.toLowerCase();
    if (/\b(optimi[sz]e|upgrade|tweak|rebuild|power (it |this )?(up|down)|make (it|this|the deck)|turn (it|this|the deck) into|tune)\b/.test(t)) return true;
    // "What should I cut?" is a question; "Cut the weak ramp" is a request.
    if (/^(what|which|how|why|is|are|does|do|when|who|where|should|could)\b/.test(t)) return false;
    return /\b(faster|slower|stronger|weaker|swap|replace|lower the bracket|raise the bracket|more (consistent|interaction|ramp|removal|draw))\b/.test(t);
  }

  /** A bracket (or, outside Commander, power level) the message names ("bracket 3", "B4", "cEDH", "power 4"), if any. */
  function namedBracket(text: string): number | null {
    const t = text.toLowerCase();
    if (format !== 'commander') {
      const p = /\bpower(?: level)?\s*([1-5])\b/.exec(t);
      return p ? Number(p[1]) : null;
    }
    if (/\bcedh\b/.test(t)) return 5;
    const m = /\b(?:bracket|b)\s*([1-5])\b/.exec(t);
    if (m) return Number(m[1]);
    for (const b of BRACKETS) if (new RegExp(`\\b${b.name.toLowerCase()}\\b`).test(t)) return b.n;
    return null;
  }

  function startRun(n: number) {
    target = n;
    pending = null;
    if (format === 'canlander') rebuildHighlander(n);
    else run();
  }

  /** Highlander has no commander for the optimizer to study: rebuild with the Highlander builder instead. */
  async function rebuildHighlander(power: number) {
    job = { id: 0, kind: 'highlander_rebuild', subject: name, status: 'running', percent: 0, step: 'Starting', detail: '', log: [], result: null, error: null };
    scrollChat();
    try {
      const res = await runJob<{ name: string; kept: string[] }>('highlander_rebuild', { name, power, message: brief }, (j) => (job = j), 1000);
      rebuilt = res.name;
      note(
        `Rebuilt in this deck's colors and play styles at power ${power}${res.kept.length ? `, keeping ${res.kept.join(', ')}` : ''}. ` +
          `It's saved as a new deck, "${res.name}"; this one isn't changed.`,
      );
    } catch (err) {
      app.error(err);
    } finally {
      job = null;
      scrollChat();
    }
  }

  async function send() {
    const text = message.trim();
    if (!text) return;
    if (wantsChanges(text)) {
      // Change requests go to the optimizer; only they need a bracket.
      chat = [...chat, { role: 'user', content: text }];
      message = '';
      brief = text;
      const n = namedBracket(text);
      if (n) {
        note(format === 'canlander' ? `Rebuilding at power ${n}.` : `Rebuilding and testing for ${format === 'commander' ? 'Bracket' : 'power'} ${n}.`);
        startRun(n);
      } else {
        pending = text;
        note(
          format === 'canlander'
            ? 'I can rebuild this Highlander deck in its colors and play styles, keeping any card you named. Which power level should I aim for?'
            : `I can rebuild this deck and goldfish-test the versions. Which ${levelWord} should I aim for?`,
        );
      }
      return;
    }
    chat = [...chat, { role: 'user', content: text }];
    message = '';
    thinking = true;
    scrollChat();
    try {
      const res = await runJob<{ chat: ChatMessage[] }>('agent_chat', { name, message: text }, (j) => {
        const grew = (j.partial?.length ?? 0) !== (live?.partial?.length ?? 0) || j.log.length !== (live?.log.length ?? 0);
        live = j;
        if (grew) scrollChat();
      }, 400);
      chat = res.chat;
    } catch (err) {
      app.error(err);
      chat = chat.slice(0, -1);
      message = text;
    } finally {
      thinking = false;
      live = null;
      scrollChat();
    }
  }

  /** A job log line without its "NN% " prefix. */
  function stepLine(line: string): string {
    return line.replace(/^\s*\d+%\s*/, '');
  }

  async function clearChat() {
    chat = (await api<{ chat: ChatMessage[] }>('agent_chat_clear', { name })).chat;
    notes = [];
    pending = null;
  }

  async function save(iteration?: number) {
    try {
      const res = await api<{ name: string }>('agent_save', { name, iteration });
      app.toast(`Saved as ${res.name}.`, 'success');
      onsaved(res.name);
    } catch (err) {
      app.error(err);
    }
  }

  const STARTERS = [
    'How do I mulligan this deck?',
    'How does this deck win?',
    'Which cards are the weakest?',
    'What are the combos?',
    'Upgrade this deck',
    'Make it win faster',
  ];

  /** The chat can use an AI model: the built-in one is running, or an online API key is set. */
  let chatAi = $derived(!!ai && (ai.running || (ai.config?.provider === 'api' && ai.config?.has_api_key) || ai.config?.provider === 'ollama'));

  /** Escape, then a little Markdown: **bold**, _italic_, `code`, bullet and numbered lists. */
  function md(text: string): string {
    const esc = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    const inline = (t: string) =>
      t
        .replace(/\*\*(.+?)\*\*/g, '<b>$1</b>')
        .replace(/(^|[\s(])_(.+?)_(?=[\s).,!?:;]|$)/g, '$1<i>$2</i>')
        .replace(/`([^`]+)`/g, '<code>$1</code>');
    const out: string[] = [];
    let list: 'ul' | 'ol' | null = null;
    for (const raw of esc.split('\n')) {
      const line = raw.trimEnd();
      const bullet = /^\s*[-*•]\s+(.*)$/.exec(line);
      const num = /^\s*\d+[.)]\s+(.*)$/.exec(line);
      const kind = bullet ? 'ul' : num ? 'ol' : null;
      if (list && kind !== list) {
        out.push(`</${list}>`);
        list = null;
      }
      if (kind) {
        if (!list) {
          out.push(`<${kind}>`);
          list = kind;
        }
        out.push(`<li>${inline((bullet ?? num)![1])}</li>`);
      } else if (line.trim()) {
        out.push(`<p>${inline(line)}</p>`);
      }
    }
    if (list) out.push(`</${list}>`);
    return out.join('');
  }

  function turn(n: number | null | undefined) {
    return n ? `turn ${n}` : '—';
  }

  let iterations = $derived<any[]>(agent?.top_iterations?.length ? agent.top_iterations : agent ? [{ round: 0, detected_bracket: agent.target_bracket, objective_score: null, deck: agent.best, testing: agent.final_testing, best: true }] : []);
</script>

<section class="chat panel">
  <div class="row chat-head">
    <h2>Deck agent</h2>
    <span class="spacer"></span>
    {#if chat.length || notes.length}<button class="btn ghost small" onclick={clearChat}>Clear</button>{/if}
  </div>
  <div class="messages" bind:this={chatBox}>
    {#if !chat.length && !notes.length}
      <p class="small muted">
        Ask anything about this deck (what a card does, how it wins, how to mulligan, its combos, mana or bracket), or ask
        for changes ("upgrade it", "make it win faster", "turn it into stax"). For changes, the agent rebuilds the deck,
        goldfish-tests each version and shows the best ones. Your deck isn't changed until you save one.
      </p>
      <div class="starters">
        {#each STARTERS as q (q)}
          <button class="chip" onclick={() => { message = q; send(); }}>{q}</button>
        {/each}
      </div>
    {/if}
    {#each chat as m, i (i)}
      {#if m.role === 'assistant'}
        <div class="msg assistant">{@html md(m.content)}</div>
      {:else}
        <div class="msg user">{m.content}</div>
      {/if}
      {#each notes.filter((n) => n.after === i + 1) as n, j (j)}<div class="msg assistant">{n.content}</div>{/each}
    {/each}
    {#if thinking}
      <div class="msg assistant thinking" aria-live="polite">
        <ul class="steps small">
          {#each (live?.log ?? []).map(stepLine) as l, i (i)}
            <li class:current={i === (live?.log.length ?? 0) - 1 && !live?.partial}>{l}</li>
          {:else}
            <li class="current">Thinking…</li>
          {/each}
        </ul>
        {#if live?.partial}<div class="partial">{@html md(live.partial)}</div>{/if}
      </div>
    {/if}

    {#if pending}
      <div class="msg assistant ask">
        <div class="starters">
          {#each levels as b (b.n)}
            {@const now = format === 'commander' && b.n === bracket}
            <button class="chip" class:current={now} onclick={() => startRun(b.n)} title={b.blurb}>
              {b.n} · {b.name}{now ? ' (now)' : ''}
            </button>
          {/each}
        </div>
        {#if format !== 'canlander'}
        <label class="field depth"><span class="small dim">Testing depth</span>
          <select class="input" bind:value={intensity}>
            <option value="quick">Quick (1 round)</option>
            <option value="standard">Standard (3 rounds)</option>
            <option value="thorough">Thorough (5 rounds)</option>
            <option value="exhaustive">Exhaustive (8 rounds)</option>
          </select>
        </label>
        {/if}
      </div>
    {/if}

    {#if rebuilt && !job}
      <div class="msg assistant">
        <button class="btn small" onclick={() => onsaved(rebuilt!)}>Open “{rebuilt}”</button>
      </div>
    {/if}

    {#if job}
      <div class="msg assistant running">
        <div class="row"><b>{job.step}</b><span class="spacer"></span><span class="num small">{job.percent}%</span></div>
        <div class="progress"><i style:width="{job.percent}%"></i></div>
        <p class="small dim">{job.detail}</p>
        <ol class="log small">{#each job.log.slice(-6) as l, i (i)}<li>{l}</li>{/each}</ol>
      </div>
    {:else if agent && format !== 'canlander'}
      <div class="msg assistant results-msg">
        <b>Agent results</b> <span class="small dim">· ranked at Bracket {agent.target_bracket}</span>
        <ol class="results">
          {#each iterations as it, i (i)}
            {@const g = it.testing?.goldfish}
            <li class="panel res">
              <div class="row">
                <b>{i === 0 ? 'Best version' : `Round ${it.round}`}</b>
                <span class="badge">Bracket {it.detected_bracket}</span>
                <span class="spacer"></span>
                <button class="btn small" onclick={() => save(it.best ? undefined : i)}>Save as new deck</button>
              </div>
              {#if g}
                <div class="stats small">
                  <span>Most likely win: <b>{turn(g.most_likely_combo_turn)}</b></span>
                  <span>Median win: <b>{turn(g.median_combo_turn)}</b></span>
                  <span>Keepable hands: <b class="num">{Math.round(g.keepable_pct)}%</b></span>
                  {#if it.stax?.median_lock_turn}<span>Median lock: <b>{turn(it.stax.median_lock_turn)}</b></span>{/if}
                </div>
              {/if}
            </li>
          {/each}
        </ol>
        {#if agent.rounds?.length}
          <details>
            <summary class="small dim">What the agent tried ({agent.rounds.length} rounds)</summary>
            <ul class="rounds small">
              {#each agent.rounds as r (r.round)}
                <li>
                  <b>Round {r.round}</b> {r.accepted ? '· kept' : '· rejected'} — {r.summary}
                  {#if r.added_cards?.length}<div class="add">+ {r.added_cards.join(', ')}</div>{/if}
                  {#if r.removed_cards?.length}<div class="rem">− {r.removed_cards.join(', ')}</div>{/if}
                </li>
              {/each}
            </ul>
          </details>
        {/if}
      </div>
    {/if}
  </div>
  {#if ai && !chatAi}
    <div class="small dim ai-note row">
      {#if ai.uses_built_in && !ai.models?.some((m: any) => m.installed)}
        <span>No AI model is downloaded, so answers come from Commander Lab's own analysis. Download one in Settings → AI model.</span>
      {:else if ai.available}
        <span>No AI model is running, so answers come from Commander Lab's own analysis.</span>
        <button class="btn small" onclick={startAi} disabled={starting}>{starting ? 'Starting…' : 'Start AI model'}</button>
      {:else}
        <span>Answers come from Commander Lab's own analysis. For open-ended questions, add an online AI key in Settings → AI model.</span>
      {/if}
    </div>
  {/if}
  <form class="composer" onsubmit={(e) => { e.preventDefault(); send(); }}>
    <textarea
      class="input"
      rows="2"
      bind:value={message}
      placeholder="Ask about the deck, or ask for changes: “upgrade it”, “make it faster at bracket 3”…"
      onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); } }}
    ></textarea>
    <button class="btn primary icon" aria-label="Send" disabled={thinking || !!job || !message.trim()}><Icon name="play" size={16} /></button>
  </form>
</section>

<style>
  .running,
  .ask,
  .results-msg {
    display: grid;
    gap: 0.5rem;
    width: 100%;
  }
  .depth {
    max-width: 240px;
  }
  .thinking {
    display: grid;
    gap: 0.4rem;
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.15rem;
    color: var(--muted);
  }
  .steps li::before {
    content: '✓ ';
    color: var(--ok);
  }
  .steps li.current::before {
    content: '… ';
    color: var(--brass);
  }
  .steps li.current {
    color: var(--text-2);
  }
  .partial {
    border-top: 1px solid var(--line-soft);
    padding-top: 0.4rem;
  }
  .chip.current {
    border-color: var(--brass);
  }
  .log {
    margin: 0;
    padding-left: 1.1rem;
    color: var(--muted);
  }
  .results {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.6rem;
  }
  .res {
    padding: 0.8rem 1rem;
    display: grid;
    gap: 0.4rem;
  }
  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem 1.2rem;
    color: var(--text-2);
  }
  .rounds {
    margin: 0.5rem 0 0;
    padding-left: 1.1rem;
    display: grid;
    gap: 0.4rem;
  }
  .add {
    color: var(--ok);
  }
  .rem {
    color: var(--danger);
  }
  .chat {
    display: grid;
    grid-template-rows: auto 1fr auto;
    height: min(80vh, 820px);
    max-width: 980px;
    margin: 0 auto;
    overflow: hidden;
  }
  .chat:has(.ai-note) {
    grid-template-rows: auto 1fr auto auto;
  }
  .starters {
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
    cursor: pointer;
    font: inherit;
    font-size: 0.85rem;
    font-weight: 700;
    text-align: left;
  }
  .chip:hover {
    border-color: var(--brass);
    color: var(--brass-strong);
  }
  .ai-note {
    margin: 0;
    padding: 0.5rem 1rem 0;
    gap: 0.6rem;
  }
  .msg.assistant {
    white-space: normal;
  }
  .msg.assistant :global(p) {
    margin: 0 0 0.45rem;
  }
  .msg.assistant :global(p:last-child),
  .msg.assistant :global(ul:last-child),
  .msg.assistant :global(ol:last-child) {
    margin-bottom: 0;
  }
  .msg.assistant :global(ul),
  .msg.assistant :global(ol) {
    margin: 0 0 0.45rem;
    padding-left: 1.2rem;
  }
  .chat-head {
    padding: 0.8rem 1rem 0.4rem;
  }
  .messages {
    overflow: auto;
    padding: 0.5rem 1rem;
    display: grid;
    gap: 0.6rem;
    align-content: start;
  }
  .msg {
    padding: 0.55rem 0.8rem;
    border-radius: var(--r-md);
    white-space: pre-wrap;
    max-width: 92%;
    font-size: 0.93rem;
  }
  .msg.user {
    justify-self: end;
    background: color-mix(in srgb, var(--brass) 18%, var(--panel));
  }
  .msg.assistant {
    background: var(--panel-2);
  }
  .composer {
    display: flex;
    gap: 0.5rem;
    padding: 0.75rem;
    border-top: 1px solid var(--line-soft);
    align-items: end;
  }
  .composer textarea {
    resize: none;
  }
  @media (max-width: 700px) {
    .chat {
      height: 75vh;
    }
  }
</style>
