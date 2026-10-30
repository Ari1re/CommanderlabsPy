# Commander Lab

A local-first, clickable Magic: The Gathering Commander deck workbench written in Rust.

## Download & run

Every push to `main` is built automatically by GitHub Actions and published on the
[Releases](../../releases) page:

| Platform | File | How to run |
|---|---|---|
| Windows | `CommanderLab-windows-x86_64.zip` | Unzip anywhere, double-click `CommanderLab.bat` (or the `.exe`). Decks live next to the exe. |
| Linux (any distro, NixOS included) | `CommanderLab-linux-x86_64.sh` | `sh CommanderLab-linux-x86_64.sh` — installs to `~/.local/share/commander-lab/app` and starts. Static musl binary, no dependencies. Decks live in `~/.local/share/commander-lab`. |
| Android 7+ | `CommanderLab-android.apk` | Install the APK. The full app runs in a built-in terminal; decks, card cache and exports stay in the app's private storage. Each build installs over the previous one. |

The local AI backend (llama.cpp / Ollama / API) is optional. If it is not available,
Commander Lab starts anyway with the deterministic generator, analysis, MPC and
exports. Set `COMMANDER_LAB_NO_AI=1` to skip it on purpose. Android never starts a
local model.

### Building from source

```sh
cargo build --release
cargo test --release
```

The workflow lives in `.github/workflows/build.yml`. The Android app is in `android/`;
it hosts the Rust binary (packaged as `libcommanderlab.so`) in a Termux terminal view.

#### Android signing

APKs are signed with the committed `android/commanderlab-ci.keystore` so updates
install over each other. To use your own key, add the repository secrets
`ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and
`ANDROID_KEY_PASSWORD`. (Switching keys once requires uninstalling the old APK.)

## 0.21.0 — MPC Autofill tab, Moxfield export, deck integrity gate, cross-platform builds

### MPC tab (deck dashboard → `MPC`)

Turns the analyzed deck into a MakePlayingCards proxy order using MPC Autofill.

- `F` / Enter / **SEARCH** — searches the MPC Autofill backend for every card face
  (double-faced cards get a matching back slot). Results are ranked by your
  preferred sources, then DPI.
- `[` / `]` — cycle art for the selected card; `V` toggles front/back; the right
  panel previews the actual MPC image.
- `T` foil, `O` card stock, `B` include/exclude sideboard.
- `X` / **EXPORT XML** — writes `exports/<deck>.mpc.xml` (MPC Autofill desktop tool,
  or import on mpcfill.com) and always writes `exports/<deck>.mpc.txt` for the
  mpcfill.com text box, even if the backend was unreachable.
- Settings live in `data/mpc_config.json`: `backend_url`, `cardback_id`,
  `preferred_sources`, `minimum_dpi`, `stock`, `foil`, `include_sideboard`,
  `max_candidates`.

### Moxfield export

`M` on any deck tab (or **MOXFIELD** in the MPC tab) writes
`exports/<deck>.moxfield.txt` and copies it to the clipboard. It uses Moxfield's
`Commander` / `Deck` / `Sideboard` import format with `(SET) collector#` on each
line, so Moxfield shows the same printings. Press **UPDATE** once after upgrading
so the card database records printings.

### Deck integrity gate

Every generated deck and AI upgrade now passes an integrity test before it is
handed to you:

- **Hard rules, auto-repaired and then re-verified:** exactly 100 cards,
  singleton (respecting "any number of" cards), colour identity, Commander
  legality, and commanders not duplicated in the 99. Combo pieces and commanders
  are never trimmed.
- **Bracket re-lock:** if repairs changed the list, the requested bracket is
  enforced again and the integrity test repeats.
- **Soft checks, reported in the deck notes:** land count, ramp, card draw,
  interaction and a 400-hand goldfish keepability test.

A list that still fails a hard rule is rejected rather than saved.

## What 0.4 adds

- Card artwork preview anywhere Commander Lab presents clickable card names.
- Scryfall daily price snapshots for USD, foil USD, EUR, foil EUR, and MTGO tix.
- Storefront buttons using Scryfall purchase links for TCGplayer, Cardmarket, and Cardhoarder.
- Commander/deck archetype detection using a local EDHREC-style theme taxonomy plus Scryfall's EDHREC popularity rank.
- Cut/add recommendations now score archetype fit, role fit, mana efficiency, and EDHREC rank.
- Commander bracket estimate with Game Changers, tutors, fast mana, extra turns, land denial, and Commander Spellbook combo tags.
- Dedicated heuristic GOLDFISH tab.
- Sideboard detection and a MAINBOARD/SIDEBOARD browser toggle.
- Main-menu REFRESH and NEW DECK buttons.
- NEW DECK supports:
  - pasted decklist
  - best-effort Moxfield URL import
  - locally generated optimized Commander shell
- Bracketed paste support for multiline decklists.
- All RGB/color values remain centralized in `src/theme.rs`.

## Scryfall data

Commander Lab uses the current Scryfall Oracle Cards bulk format:

```text
/bulk-data/oracle-cards
        ↓
jsonl_download_uri
        ↓
.jsonl.gz
        ↓
GzDecoder
        ↓
JSON Lines streamed one card at a time
        ↓
SQLite
```

Press UPDATE to refresh card data, prices, Game Changer flags, image URLs, and marketplace links.

Prices are snapshots from the Scryfall bulk data at the time of your last update; they are not checkout quotes.

## First run / upgrade

```powershell
cargo clean
cargo build --release
cargo test
cargo run --release
```

If upgrading from an older build, the SQLite schema has migration statements for the new price/store/Game Changer fields. If you want the cleanest rebuild, delete `data/cards.sqlite` once before pressing UPDATE.

## Main menu

- Click a deck: analyze it.
- `REFRESH`: rescan `./decks`.
- `NEW DECK`: paste/import/generate.
- `UPDATE`: refresh Scryfall data.
- `QUIT`: exit.

Keyboard:

- Up/Down or J/K — select deck
- Enter — analyze
- R — refresh
- N — new deck
- U — update data
- Q — quit

## New deck menu

### Paste a deck

Choose `PASTE DECKLIST`, enter a deck name, then paste the list.

- F2 or Ctrl+Enter saves the deck.
- Sideboard sections are preserved.

### Import Moxfield

Choose `MOXFIELD URL` and paste the deck URL.

Commander Lab tries current community-observed API routes (`v3` then `v2`) and understands both direct zones and `boards.*.cards` layouts. Moxfield does not provide a documented stable public deck API and may block automated requests through Cloudflare. If import fails, use Moxfield's text export with Paste Decklist.

### Generate optimized shell

Choose `GENERATE OPTIMIZED` and enter a commander name.

The generator is deterministic/local. It uses color identity, card roles, Scryfall EDHREC rank, commander-derived archetype tags, mana efficiency, and baseline role targets to create a legal 99-card library plus commander. Treat it as a strong starting shell, not a solved cEDH list.

## Decklist formats

The loader accepts plain lists, common Moxfield exports, MDFCs, finish markers, and sideboards.

Plain:

```text
1 Sol Ring
1 Arcane Signet
...

1 Kefka, Court Mage
```

Moxfield-style:

```text
1 Casal, Lurkwood Pathfinder / Casal, Pathbreaker Owlbear (SLX) 29
1 Eternal Witness (F08) 3 *F*
28 Forest (SLD) 2080 *F*

SIDEBOARD:
1 Arcane Signet (SLD) 2538
```

Single-slash multiface names are normalized to Scryfall's canonical ` // ` form.

Commander detection priority:

1. Commander / Command Zone section
2. `*CMDR*` / `[Commander]` marker
3. filename match against the full card or one multiface face
4. separated 1–2 card final block after a large main deck
5. otherwise no unsafe guess

The supplied Kefka and Casal exports are included in `tests/fixtures` and exercised by parser tests.

## Analysis tabs

- OVERVIEW — legality, bracket, curve/role snapshots, detected archetype tags
- CARDS — mainboard or sideboard browser with price + image preview
- MANA — colored-source and pip-demand BarCharts
- CURVE — native Ratatui Chart + Sparkline
- ROLES — LineGauges for ramp/draw/interaction/tutors/etc.
- LEGALITY — Commander construction checks
- CUT / ADD — archetype-aware optimization candidates
- BRACKET — estimated Commander bracket and reasons
- SIM — 50,000-run draw/resource Monte Carlo simulation
- GOLDFISH — 10,000 heuristic pilot runs using known combo pieces when available
- COMBOS — Commander Spellbook combo lookup
- HELP — controls and caveats

## Card preview and stores

Clickable card names feed the shared card-preview panel. The panel shows:

- card image
- mana cost/type/Oracle text
- EDHREC rank
- price snapshot
- TCGplayer button
- Cardmarket button
- Cardhoarder button

Images are cached in `data/images/` after first use.

## Archetype tags

Commander Lab intentionally does **not** scrape EDHREC pages. It maintains a local taxonomy inspired by common Commander/EDHREC themes such as:

- Aristocrats
- Artifacts
- Auras
- Big Mana
- Blink
- Counters
- Enchantments
- Equipment
- Graveyard
- Landfall / Lands
- Legends
- Lifegain
- Mill
- Proliferate
- Reanimator
- Sacrifice
- Spellslinger
- Stax
- Storm
- Tokens
- Treasure
- Typal
- Voltron
- Wheels

The commander receives a heavier archetype weight, then the deck's cards refine the detected profile. Suggestions combine this profile with Scryfall's `edhrec_rank` popularity field.

## Bracket estimate

The local estimate looks at:

- Scryfall `game_changer`
- tutors
- fast mana
- extra-turn cards
- mass-land-denial patterns
- Commander Spellbook combo bracket tags, when COMBOS/GOLDFISH has been run

Bracket 5 is intentionally labeled as a profile estimate because cEDH is partly about competitive intent and metagame expectations, not only card contents.

## Goldfish disclaimer

The GOLDFISH tab is a real randomized simulation, but the pilot is heuristic rather than a full Magic rules engine. It models opening-hand quality, land development, cheap ramp, card draw, tutors, and assembly of Commander Spellbook combo lines. Use it for A/B comparisons and consistency estimates, not tournament-rules adjudication.

## Theme

Every concrete UI color lives in:

```text
src/theme.rs
```

No `Color::Rgb(...)` values are constructed elsewhere in `src/`.


## 0.5.0 additions

- Clickable EDHREC-style archetype tags. Selecting one shows matching deck
  cards, and those names feed the shared art/price/store preview.
- Combo card lists remain clickable.
- GOLDFISH is now a manual playtest sandbox with Hand, Battlefield and
  Graveyard zones. Click any card to preview it.
- Playtest controls: NEW HAND, DRAW, NEXT TURN, MULLIGAN, PLAY, TO GRAVE,
  RETURN.
- Heuristic goldfish percentages remain visible as reference data.
- SIM remains a separate Monte Carlo resource-consistency tab.
- Added THEME page and main-menu THEME button.
- Click any theme swatch, type #RRGGBB, and press Enter.
- Themes persist in `data/theme.json` and load automatically.
- Every RGB literal remains centralized in `src/theme.rs`.


## 0.5.2 Theme navigation

Theme customization now lives at the application level rather than inside an
individual deck.

Open **THEME** from the main deck-library screen. It opens a dedicated theme
screen. Press **Esc** or click **BACK** to return to the library.


## 0.6.0

### Partner commander detection and generation

Shared pairing logic now recognizes Partner, Partner With, Friends Forever,
Choose a Background + Background, and Doctor's Companion + Time Lord Doctor.

Generator input may be:

```text
Tymna the Weaver + Kraum, Ludevic's Opus
```

When only one commander is entered and it advertises a pairing mechanic,
Commander Lab attempts to choose an appropriate partner automatically.

Two-commanders decks correctly generate a 98-card library.

### Game-plan generator

Generated decks now deliberately build around:

- commander/archetype tags,
- one or two combo win packages,
- ramp,
- card advantage,
- interaction,
- tutors,
- protection,
- recursion,
- and an appropriate mana base.

Commander Spellbook is used to verify generated combo lines and can supply a
missing piece for a one-card-away line.

Each generated deck gets a sidecar:

```text
<deck name>.plan.md
```

containing its strategy and combo packages.

### Sideboard suggestions

In CUT / ADD:

1. click a cut candidate or add suggestion,
2. click **SIDEBOARD SELECTED**.

A CUT is moved from the main deck to Sideboard. An ADD is added directly to
Sideboard. The deck file is saved immediately.


## 0.6.1 — bracket-targeted generation + deck deletion

### Generated deck bracket selection

Generate Optimized now uses two steps:

1. enter the commander or legal commander pair,
2. choose Bracket 1–5.

The chosen bracket changes generation behavior, including:

- Game Changer density,
- tutor density,
- fast-mana preference,
- number and speed of seeded combo packages,
- efficiency weighting.

Bracket 1 deliberately avoids deterministic compact combo seeding because
that would work against the requested Exhibition target. Higher brackets
progressively allow more focused and faster win packages.

### Remove decks from the library

The main deck-library screen now has:

```text
DELETE
```

Select a deck and click DELETE (or press `D` / Delete), then confirm.

Commander Lab removes the selected `.txt` deck file and its generated
`.plan.md` sidecar when present, then refreshes the library.


## 0.7.0 — reference data, Archidekt, RGB pickers, clipboard and price

- Overview shows Scryfall USD total for the main deck and sideboard.
- NEW DECK imports public Archidekt URLs.
- Generator uses a small 24-hour, rate-limited cache of public reference decks
  from Archidekt and best-effort Moxfield search.
- Bracket 5 additionally tries cEDH Decklist Database-linked Moxfield lists.
- Reference frequency is an extra signal, not a replacement for Commander
  rules, archetype fit, structural role targets, bracket limits or Spellbook.
- Theme swatches open an RGB slider picker instead of a hex-only field.
- COPY exports Commander / Deck / Sideboard sections to the system clipboard.


## 0.8.0 — guided coherent generation

Generated decks now use a guided workflow:

```text
type approximate commander name
        ↓
auto-correct / auto-complete
        ↓
partner detection
        ↓
choose one of the commander's strongest inferred themes
        ↓
choose Bracket 1–5
        ↓
live staged generation progress
        ↓
deck + game-plan file
```

### Commander auto-correction

Commander input no longer requires an exact Scryfall spelling. Commander Lab
scores exact matches, prefixes, token overlap, edit distance and local EDHREC
rank to select the most likely commander. The modal also shows likely matches
while typing.

When a confident correction is made, the status bar explicitly says:

```text
Assuming you meant: <canonical commander name>
```

Partner/paired commander rules still run after name correction.

### Theme selection

Before bracket selection, Commander Lab presents up to six strong themes
inferred from the commander text plus the depth of legal cards in the local
card pool.

The selected theme becomes a hard generation signal rather than a cosmetic
label.

### Coherence rules

The generator now rejects flexible cards that have no reason to be in the
deck. A nonland card must satisfy at least one of:

- directly supports the chosen theme,
- fills a structural role such as ramp/draw/interaction/tutor/protection/
  recursion,
- appears in a relevant public reference deck,
- belongs to the selected combo/win package.

The generator targets a substantial number of direct theme-support cards
before spending flexible slots.

Combo templates are no longer pulled globally just because the colors allow
them. Local seeded combos must either match the chosen theme or be supported
by matching reference lists.

### Progress screen

Generation runs in a worker thread so the TUI stays responsive and repaints a
live progress gauge. The user sees the current stage, including:

- resolving commander,
- locking theme,
- loading public references,
- choosing win packages,
- building the theme engine,
- filling structural roles,
- constructing the mana base,
- checking Commander Spellbook,
- validating coherence,
- final save.


## 0.8.1 — expanded EDHREC theme selection

The guided generator now shows up to **12** commander themes instead of six.

Theme priority is now:

1. EDHREC commander themes/tags, in EDHREC order,
2. direct commander-text archetype detection,
3. legal-card-pool support,
4. local fallback themes only when necessary.

EDHREC commander JSON is fetched from:

```text
https://json.edhrec.com/pages/commanders/<slug>.json
```

and cached for 24 hours under:

```text
data/edhrec_cache/
```

The theme picker displays twelve choices in two clickable columns so
commanders with several viable archetypes expose more than just their first
few strategies.


## 0.8.2 — actual EDHREC top-10 tags

EDHREC commander tags are now read from the current JSON field:

```text
panels.taglinks[]
```

Each entry supplies:

```text
value
slug
count
```

Commander Lab sorts those tags by `count` descending and uses the actual
EDHREC top ten as the first ten generation-theme choices.

Local inference can supplement the list but cannot push an EDHREC top-ten tag
out of those first ten positions.

Existing cached raw EDHREC JSON remains valid; no cache wipe is required.


## 0.8.3 — themes tailor play style without excluding cards

The selected generation theme is now a **soft preference**, not a card filter.

Theme choice affects:

- scoring bonuses,
- preferred engine density,
- combo ordering,
- reference weighting,
- game-plan wording.

It does not remove otherwise legal cards from consideration.

Generic interaction, protection, mana acceleration, tutors, lands, efficient
staples, off-theme combo pieces, and other useful cards remain eligible when
they improve the complete deck.

Cards with no theme, structural-role, or reference support receive only a
small coherence penalty rather than being excluded.


## 0.9.0 — commander-normal-build intelligence

The generator now treats the commander's normal EDHREC shell as the baseline
and the selected user theme as a play-style modification of that baseline.

New build signals, in descending importance:

1. the commander's actual rules text,
2. the commander's top EDHREC tags,
3. commander-specific EDHREC High Synergy / Top Cards / category lists,
4. matching Moxfield / Archidekt reference decks,
5. selected user play-style theme,
6. structural roles and bracket requirements,
7. global EDHREC rank as a secondary tiebreaker.

Commander-specific EDHREC cards are added to the candidate pool even when
their global rank falls outside the generic top-card cutoff.

The generator tracks both commander-engine density and EDHREC-normal-build
density. For a commander such as Ulalek, whose text explicitly cares about
Eldrazi spells and whose dominant EDHREC tag is Eldrazi, Eldrazi cards receive
a strong engine score until the deck contains a meaningful Eldrazi package.

### Combo relevance

Generic color-compatible combos are no longer automatically seeded.

A combo seed must be supported by at least one of:

- the selected play-style tag,
- one of the commander's normal EDHREC core tags,
- commander-specific EDHREC card data,
- matching public reference decks.

Commander Spellbook one-card-away completions and reported final combos are
also filtered for commander/theme/reference relevance.

This prevents a five-color commander from receiving arbitrary Oracle/
Consultation or Exquisite Blood/Sanguine Bond packages just because the color
identity permits them.

### Mana-base intelligence

Nonbasic lands are no longer sorted only by global EDHREC rank. Commander-
specific EDHREC inclusion/synergy and matching reference-deck frequency now
strongly affect mana-base ordering, allowing cards such as typal/engine lands
to beat generic lands when they are central to the commander's normal builds.


## 0.10.0 — keyboard focus, Sources, and AI Agent

### Keyboard focus

The following analysis pages are no longer mouse-only:

- **CUT / ADD** — Left/Right changes column, Up/Down changes card, Enter
  previews/selects, `B` moves/adds the selected suggestion to Sideboard.
- **SIM** — Up/Down highlights consistency metrics, Enter reruns simulation.
- **COMBOS** — Up/Down highlights combo cards and Enter previews card art/data.
- **SOURCES** — Up/Down selects a citation and Enter opens it.
- **AI AGENT** — Up/Down navigates learned cards, Enter previews, `D` reruns.

`H` / `L` always move between tabs.

### Sources tab

Generated decks save:

```text
<deck>.sources.json
```

This preserves the exact public reference URLs used during generation,
including matching Moxfield/Archidekt/cEDH DDB references when available.

The Sources tab also exposes the foundational sources:

- Scryfall
- EDHREC commander/theme pages
- Commander Spellbook
- Commander rules
- Moxfield
- Archidekt
- cEDH Decklist Database for Bracket 5

Every source row is clickable and keyboard-openable.

### AI Agent

AI Agent is a **local research/recommendation agent**, not a paid LLM API.

It refreshes and combines:

1. commander-specific EDHREC top tags,
2. commander/theme EDHREC cardlists, inclusion and synergy,
3. public Moxfield reference shells,
4. public Archidekt reference shells,
5. cEDH DDB-linked lists for Bracket 5,
6. local Scryfall legality/card information,
7. the user's current list.

It calculates:

- normal commander build identity,
- top-30 commander-core coverage,
- high-confidence missing cards,
- current cards with weak support in normal shells,
- exact research sources.

This data is advisory. It does not automatically mutate the user's deck.


## 0.11.0 — Taskii-inspired terminal UI overhaul

Commander Lab now defaults to a restrained warm terminal aesthetic:

- dark brown-black application background,
- brown panel surfaces,
- amber/orange borders and primary highlights,
- muted cream text,
- green/red/yellow reserved for semantic status,
- outlined action controls instead of Metro-style filled buttons.

### Wide-screen layout

At 150+ terminal columns the analysis workspace becomes:

```text
┌──────────────────┬────────────────────────────────────────┬──────────────────┐
│ COMMANDER LAB    │                                        │ QUICK STATUS     │
│ deck metadata    │           ACTIVE ANALYSIS TAB          │ notes / keys     │
│ archetype tags   │                                        │ research status  │
└──────────────────┴────────────────────────────────────────┴──────────────────┘
                     tabs live near the bottom
                     status + action strip
```

The library uses a similar 25 / 50 / 25 three-column composition:

```text
brand/database  |  deck library + selection  |  data/research notes
```

Narrow terminals automatically fall back to a compact single-content layout.

### Theme migration

If `data/theme.json` is exactly the old untouched Metro default palette,
Commander Lab automatically migrates it to the new warm terminal default.

Any custom theme is preserved unchanged.


## 0.12.0 — shared AI Agent intelligence

AI Agent is no longer a separate advisory algorithm. Generation and the
AI AGENT tab now consume the same `CommanderIntelligence` model.

The learned model combines:

- the commander's top EDHREC tags,
- both the base commander cardlists and the user's selected-theme cardlists,
- EDHREC inclusion and synergy,
- matching public Moxfield / Archidekt shells,
- cEDH DDB references for Bracket 5,
- commander Oracle text / engine terms,
- color identity,
- structural card roles,
- Scryfall metadata.

### Learned structural targets

Ramp, draw, interaction, tutors, wipes, protection, and recursion are no
longer fixed to the same counts for every commander.

Commander Lab examines the commander-specific EDHREC/reference sample and
learns a normal role distribution, then clamps it to safe Commander ranges and
the requested bracket.

### Shared card-support score

Every candidate receives a commander-specific support score built from:

```text
EDHREC inclusion
+ EDHREC synergy
+ EDHREC category
+ reference-deck frequency
+ commander-engine match
+ normal-shell tag match
+ chosen play-style match
+ structural usefulness
```

Cards with no learned support are not banned; they receive an uncertainty
penalty and can still win a slot on efficiency or necessity.

### Combo intelligence

A combo must now achieve a minimum support score in the same learned commander
model before it can be seeded or completed from Commander Spellbook. This
makes unrelated color-legal combos much less likely.

### Explainability

AI Agent now reports:

- model confidence,
- learned role targets,
- learned EDHREC/reference counts,
- per-card recommendation confidence,
- why each add/cut is supported.

Generated `.plan.md` files also record the AI Agent confidence and learned
role targets used during generation.


## 0.13.0 — explainable AI, deep simulations, combo explorer, named themes

### AI Agent decision trace

AI Agent now displays observable evidence and decisions while it works:
EDHREC shell, public-reference support, learned role targets, per-card scores,
confidence, and simulation results. This is an explainable decision trace, not
hidden chain-of-thought.

### Deep testing intensity

```text
QUICK        10,000 draw simulations +   5,000 goldfish games
STANDARD     75,000 draw simulations +  30,000 goldfish games
THOROUGH    300,000 draw simulations + 125,000 goldfish games
EXHAUSTIVE 1,000,000 draw simulations + 500,000 goldfish games
```

Goldfish testing now records exact known-combo assembly turns T1–T8, earliest
observed winning turn, most-likely winning turn, median winning turn, and
cumulative win probability by turn.

These are heuristic comparative estimates, not a complete Magic rules engine.

### Commander Spellbook explorer

The COMBOS page now selects whole Spellbook variants. Hover or use Up/Down to
inspect cards, result, prerequisites, steps/notes, bracket, and the Spellbook
URL. `O` or OPEN SPELLBOOK opens the selected variant.

### Named themes

Built-in palettes include Lambda, Dracula, Gruvbox Dark, Nord, Tokyo Night,
and Solarized Dark. Edit colors, then SAVE AS... (or `N`) to create/update a
named user theme stored in `data/themes.json`.

### Generation trace

Deck generation now retains and displays its progress/evidence messages as a
BUILD DECISION TRACE rather than only showing the latest status line.


Direct Spellbook combo pages use the current `/combo/<id>/` route when the
Spellbook API returns a combo identifier. The combo panel also attempts to
surface initial card state, prerequisites, steps/notes, results, and bracket.

AI Agent's win-speed test records both exact turn percentages and cumulative
probability through turn 8. "Earliest win" means earliest observed assembly of
a known complete Commander Spellbook line in the heuristic sample.


## 0.14.0 — Neon Slate / application-shell redesign

The default visual system now follows a dark blue-violet application shell
with cyan focus borders, magenta secondary accents, and yellow/orange status
emphasis.

### New default theme

The new built-in default is **Neon Slate**.

The previous brown/amber style remains available as **Lambda**.

Other named themes remain available:

- Dracula
- Gruvbox Dark
- Nord
- Tokyo Night
- Solarized Dark

Exact saved Lambda defaults are automatically migrated to Neon Slate. Any
user-customized palette remains untouched.

### Semantic theme roles

The theme editor no longer exposes legacy implementation names such as
`BLUE`, `CYAN`, `MAGENTA`, and `PURPLE`.

The same serialized palette remains backward-compatible, but the UI now calls
them:

```text
APP BACKGROUND
PANEL SURFACE
SELECTED SURFACE
PRIMARY TEXT
MUTED TEXT
PRIMARY ACCENT
ACTIVE / FOCUS
SUCCESS / POSITIVE
ACTION / ORANGE
WARNING / EMPHASIS
SECONDARY ACCENT
DANGER / ERROR
AI / AI AGENT
```

MTG mana colors keep their literal White / Blue / Black / Red / Green names.

### Layout

Wide layouts now use the application structure from the new visual reference:

```text
application header / current status / clock
summary cards
filter / navigation strip
main data pane | selected-item detail pane
progress & activity | quick actions
compact keyboard footer
```

The deck-analysis workspace uses the same shell, while the active analysis
page remains interactive in the main pane.


## 0.15.0 — AI Agent workspace

AI Agent is now a dedicated screen rather than a normal analysis tab.

```text
OVERVIEW
RESEARCH
SIMULATE
WIN SPEED
UPGRADE
RECOMMEND
SOURCES
TRACE
```

Dashboard and deck-library AI Agent buttons open this workspace.

### Research and simulation are separate

RESEARCH learns:

- normal EDHREC shell,
- commander/reference evidence,
- role targets,
- per-card support and confidence,
- recommendations.

SIMULATE is explicitly launched by the user and uses the selected test
intensity.

### Muldrotha-A-style win-speed testing

WIN SPEED reports:

- total games tested,
- exact wins on T1 through T8,
- raw match count for every turn,
- exact percentage for every turn,
- games with no known win by T8,
- earliest observed win,
- most-likely winning turn,
- median winning turn,
- cumulative chance of winning by T3/T4/T5/T6/T7/T8,
- known Commander Spellbook lines that produced simulated wins.

The game tester remains a deterministic heuristic goldfish/known-line model.
It models opening hands, lands, ramp, simple draw/tutor behavior and known
Spellbook line assembly. It is intended for comparative deck-speed testing,
like the Muldrotha-A iterations, rather than literal four-player match odds.

### Upgrade workspace

UPGRADE contains:

- COPY CURRENT DECKLIST,
- target Commander bracket selection,
- GENERATE AI UPGRADE.

AI Upgrade creates a new deck and never overwrites the original. It builds a
new learned shell for the same commander and target bracket, compares the
generated flexible slots with the current deck, retains sufficiently
high-support current cards, and moves displaced generated alternatives into
Sideboard/consideration.


## 0.16.0 — large reference corpus + hard bracket lock

### AI Agent reference corpus

Reference analysis now attempts to collect a much larger commander-matched
sample:

```text
Archidekt         up to 24 matching lists
Moxfield          up to 24 matching lists
cEDH DDB layer    up to 32 matching lists for Bracket 5
Final corpus       up to 60 unique decks
```

Actual counts depend on how many public matching decks are available and which
sites respond successfully. Results are cached for 24 hours.

AI Agent now compares the corpus at three levels:

1. **Card consensus** — how many reference decks contain each card.
2. **Recurring packages** — card pairs that repeatedly appear together.
3. **Combo consensus** — a large subset of the reference corpus is submitted
   to Commander Spellbook so the program can record which complete combo lines
   are actually present in real builds of that commander.

For normal research up to 36 reference decks receive the Spellbook combo scan.
For Bracket 5 research up to 50 reference decks receive the combo scan.

The RESEARCH page exposes separate panels for common cards/packages and combos
used by the reference decks.

### Generation uses the reference corpus

Generation pulls up to 500 high-frequency reference cards into its candidate
pool even when their global EDHREC rank would normally leave them outside the
generic pool.

Recurring reference-deck combos are considered before Commander Lab's static
fallback combo templates. A combo observed repeatedly in actual matching
commander lists gets a large relevance bonus, especially at Bracket 5.

### Requested bracket is now a hard contract

Previously the selected bracket only changed card scoring. That meant a user
could request Bracket 5 and still receive a list that Commander Lab's own
detector later classified as Bracket 1.

Generation now performs this final pipeline:

```text
build list
→ complete/verify combo lines
→ run bracket detector
→ power-profile correction pass if under target
→ power-down correction if over target
→ run bracket detector again
→ save only when detected bracket == requested bracket
```

Bracket 5 generation specifically reinforces the detector's competitive
baseline: Game Changers, tutor density, fast mana, and competitive combo
density. Commander Spellbook combo information is also used by the bracket
refinement layer.

If Commander Lab cannot make the deck validate at the requested bracket, it
returns a generation error explaining the final profile instead of saving a
deck under the wrong bracket.

AI Upgrade runs the same bracket validation again *after* retaining cards from
the original list, so those substitutions cannot silently lower the requested
power bracket.


## 0.17.0 — AI Optimization AI Optimization + universal AI Agent card preview

### AI Agent card preview is now global

Every AI Agent page now reserves a right-side card preview column using the
same image, card-data, current-price, and storefront controls as the rest of
Commander Lab.

The preview stays visible on:

```text
OVERVIEW
AGENT
RESEARCH
SIMULATE
SPEED
UPGRADE
RECOMMEND
SOURCES
TRACE
```

Card names exposed by AI Agent are registered as mouse targets where the
data is structured. This includes:

- learned ADD / CUT cards,
- common cards from the reference corpus,
- recurring card packages,
- combos seen in reference decks,
- known combo lines from simulation,
- cards added/removed by optimization rounds,
- Stax lock pieces.

Hovering or clicking those names feeds the common card-art / price / store
preview pane.

### Local AI Optimization Optimization Agent

AI Agent now contains a dedicated `AGENT` page and a global `AI OPTIMIZATION`
button.

This is a local orchestration/optimization agent. It does not require a paid
LLM API. It coordinates the existing Commander Lab research, generator,
bracket detector, Commander Spellbook checks, reference corpus, and simulation
engines.

The agent runs the whole optimization loop:

```text
read current deck
→ research commander + dozens of reference decks
→ learn common cards / packages / recurring combos
→ establish a baseline simulation
→ generate bracket-locked candidate
→ run search simulation
→ compare objective score
→ accept only an improvement
→ try additional commander/archetype candidates
→ repeat for configured rounds
→ run full selected-intensity validation
→ validate requested bracket again
→ offer SAVE BEST AGENT DECK
```

If the current deck is already in the requested bracket, the agent will not
accept a slower/worse generated candidate simply to produce output.

If the current deck is in the wrong bracket, only bracket-valid candidates are
eligible for comparison.

### FASTEST WIN objective

Default goal:

```text
FASTEST WIN
```

The objective rewards:

1. earliest observed known-combo win,
2. earlier most-likely winning turn,
3. earlier median winning turn,
4. higher win probability by T3 / T4 / T5 / T6,
5. opening-hand consistency.

The agent uses quick samples while searching candidates, then re-tests the
best accepted candidate at the user's selected QUICK / STANDARD / THOROUGH /
EXHAUSTIVE intensity.

The selected deck archetype/theme is tried first, followed by other leading
commander themes from EDHREC/reference intelligence.

### STAX: FASTEST LOCK objective

Selecting a Stax archetype automatically switches the agent objective to:

```text
STAX LOCK
```

It can also be toggled directly on the AGENT page.

Stax generation receives a denser lock-piece theme target and protects the
generated Stax engine from being displaced during current-deck retention.

The Stax simulator tracks common effects such as:

- Rule-of-Law / spell-count restrictions,
- cast restrictions,
- search restrictions,
- artifact/creature activated-ability denial,
- untap denial,
- mana/tax effects,
- nonbasic-land restrictions,
- hard lock commanders,
- multiple lock pieces cast in the same turn.

A pressure score of 3 is treated as a functional lock for comparative testing.
One hard denial piece can reach that threshold itself; softer taxes generally
need to stack.

The SPEED page changes to a Stax-specific report showing:

```text
exact lock count by T1–T8
no lock by T8
earliest lock
most-likely lock turn
median lock turn
cumulative chance locked by each turn
lock pieces most often deployed
```

Commander-zone Stax commanders are included in the lock timing model.

The Stax model is deliberately heuristic. It does not claim to simulate every
Magic rule or three intelligent opponents; it is intended to compare revisions
and minimize lock timing consistently.

### Agent configuration

The AGENT page contains clickable controls for:

```text
FASTEST WIN / STAX: FASTEST LOCK
Bracket 1 / 2 / 3 / 4 / 5
Quick / Standard / Thorough / Exhaustive
optimization rounds - / +
RUN AGENT
SAVE BEST AGENT DECK
```

Bracket selection remains a hard contract: the final agent deck must validate
at exactly the requested Commander Lab bracket before it can be returned.


## 0.17.1 — commander name resolver

Commander generation/autocomplete accepts full names, leading names, missing
punctuation, and small spelling errors. Examples that should resolve:

```text
Shorikai
Shorikai Genesis Engine
Shorikai, Genesis Engine

Urtet
Urtet Remnant of Memnarch
Urtet, Remnant of Memnarch
```

The resolver also corrects an older commander-eligibility bug where a type line
such as `Legendary Artifact Creature — Myr` did not match the literal phrase
`Legendary Creature`.

Eligibility is recalculated when rows are read, so an existing
`data/cards.sqlite` built by an older Commander Lab version does not need to be
deleted to benefit from this fix. Running UPDATE is still recommended whenever
you want current card data and prices.


## 0.17.2 — structured AI Agent and generation logging

Commander Lab now keeps a persistent rotating application log at:

```text
data/logs/commander-lab.log
```

When that file reaches roughly 5 MB it is rotated to:

```text
data/logs/commander-lab.previous.log
```

Every log line includes a millisecond-resolution Unix timestamp and a subsystem
scope such as:

```text
[...timestamp...] [GEN] ...
[...timestamp...] [DEEP] ...
[...timestamp...] [SIM] ...
[...timestamp...] [AGENT] ...
[...timestamp...] [AGENT-DECISION] ...
[...timestamp...] [DEEP-DECISION] ...
[...timestamp...] [REF] ...
[...timestamp...] [BRACKET] ...
```

AI Agent logs now include commander resolution, deck size, requested bracket,
EDHREC signal counts, reference-deck source counts, common cards, recurring
packages and combos, role targets, ADD/CUT ranking results, model confidence,
simulation resource statistics, turn-by-turn win metrics, known winning-line
distribution, Agent candidate scores, accepted/rejected rounds, card diffs,
Stax baseline data, and final validation.

Reference collection writes cache hits/misses, Archidekt/Moxfield/cEDH counts,
Spellbook scan size/results and final corpus statistics.

Deck generation logs candidate-pool size, selected reference combo packages,
final combo packages, theme/engine/core counts, role counts, bracket profile,
power-up/power-down correction swaps, final validated bracket, generated
combos, plan notes and output artifact paths.



## 0.18.0 — cumulative source cache + debounced commander autocomplete

### Cumulative source research

Commander Lab now treats public deck/reference data as a local research
library instead of disposable 24-hour results.

New cache layers:

```text
data/source_cache/reference_decks/
    parsed individual Archidekt/Moxfield/cEDH-linked decklists

data/source_cache/search_pages/
    cached discovery/search/API pages

data/source_cache/commander_archives/
    long-lived commander-wide deck archives shared across brackets

data/source_cache/crawl_state/
    remembers which discovery page to inspect next
```

The old 24-hour `reference_cache` remains as the fast fully-built analysis
snapshot. When it expires Commander Lab does **not** throw its old research
away. It starts from the commander archive, then uses a small network budget
to discover another page/batch and merges new decks into the archive.

Reference decks are cached by normalized source URL and reused locally. The
program therefore does not repeatedly download the same Moxfield or Archidekt
deck on every AI Agent run.

The source archive is commander-wide rather than bracket-specific. Research
collected while analyzing a commander at B2/B3/B4 is immediately useful when
that same commander is later researched at another bracket. B5 additionally
adds cEDH sources to the same archive.

The commander archive can retain up to 500 unique reference decks. The active
frequency/package analysis uses up to 120 at a time so analysis remains
responsive while older material stays available locally.

### Polite incremental crawling

Each source refresh uses a small per-host new-deck budget instead of fetching
every search result:

```text
maximum new deck fetches per source/refresh: 12
search/discovery page cache:                 8 hours
fully-built corpus snapshot:                24 hours
EDHREC JSON cache:                           72 hours
```

Archidekt and Moxfield crawl state advances through result pages over time.
Repeated AI Agent sessions therefore gradually accumulate more independent
reference decks without repeatedly requesting page 1 and the same popular
lists.

If a host is unavailable or rejects a refresh, cached search/EDHREC data is
used as a stale fallback rather than immediately retrying or discarding the
research.

### Commander Spellbook work is cached per deck

Each cached reference deck records whether Commander Spellbook analysis has
already completed.

This includes decks where Spellbook found **zero** complete combos. That
distinction matters: a valid zero-combo result is now remembered and is not
re-submitted every refresh.

Only previously-unscanned reference decks consume the next Spellbook scan
budget, letting combo knowledge accumulate as the commander archive grows.

### Existing cache migration

Existing 0.17.x `reference_cache` snapshots are reused. Their deck entries are
automatically seeded into the new per-source/commander caches rather than
forcing users to start their research library over.

### Commander autocomplete debounce

The deck-generation commander textbox no longer performs fuzzy SQLite/ranking
work while every character is being typed.

Behavior is now:

```text
keystroke
→ textbox updates immediately
→ 300 ms debounce timer starts/restarts
→ user keeps typing: no fuzzy search
→ user pauses for 300 ms
→ fuzzy commander search runs once
→ results remain cached until the text changes
```

A one-character query does not trigger the broad fuzzy scan. This keeps the
generation modal responsive even with a large local Scryfall database.


## 0.18.1 — clickable commander autocomplete

The commander-generation autocomplete list is now mouse selectable.

```text
Shorikai
  1. Shorikai, Genesis Engine     ← click

Urtet
  1. Urtet, Remnant of Memnarch  ← click
```

Clicking a suggestion fills the commander textbox. It does not immediately
advance, so partner/background pairs still work:

```text
Tymna
→ click Tymna the Weaver
→ type " + Kraum"
→ pause 300 ms
→ click Kraum, Ludevic's Opus
→ Enter
```


## 0.18.2 — runtime commander-rule validation

Commander detection no longer treats the SQLite `commander_eligible` column as
the final authority. Every path now evaluates the current card at runtime.

For Shorikai specifically the normal rule remains:

```text
Legendary Artifact — Vehicle
Oracle text: "... can be your commander."
```

A compatibility exception is also present so a stale/local database with a
missing commander-permission sentence still recognizes:

```text
Shorikai, Genesis Engine
```

The application log now contains `[COMMANDER]` entries. If another commander
fails, the log records the local type line, Commander legality, Oracle-text
length and the rule used for eligibility, making database/import problems easy
to distinguish from resolver problems.


## 0.18.3 — live generation activity, no deck-folder log

Deck generation no longer creates a `.build.log.txt` beside generated decks.

While generation is running, the modal is arranged as:

```text
generation status
progress bar
live generation activity
```

The activity panel is an in-memory live tail. As new stages arrive, the newest
messages stay visible automatically instead of being pushed below the bottom
of the panel.

The deck directory therefore contains the deck and its normal companion data,
not a transient generator-debug logfile.


## 0.19.0 — local MTG AI research/optimization layer

Commander Lab now contains a real local language-model layer for AI Agent
Agent sessions. It is deliberately **local-first** so using it does not incur
per-request API charges.

### Local model runtime

Commander Lab talks to Ollama on:

```text
http://127.0.0.1:11434
```

The first run creates:

```text
data/ai_config.json
```

Default configuration:

```json
{
  "enabled": true,
  "base_url": "http://127.0.0.1:11434",
  "model": "qwen2.5:7b-instruct",
  "temperature": 0.15,
  "timeout_seconds": 180,
  "web_research": true
}
```

If the configured model is not installed but Ollama has another model,
Commander Lab automatically uses the first installed local model.

Example setup:

```powershell
ollama pull qwen2.5:7b-instruct
ollama serve
```

If Ollama is unavailable, AI Agent does **not** stop working. The existing
deterministic research/generation/simulation agent remains active and the UI
shows `LOCAL AI: OFFLINE / FALLBACK`.

### What the local AI actually does

The model is not trusted as a Magic rules engine. Commander Lab builds a
grounded research context before it calls the model:

```text
Scryfall/local card database
Commander text + color identity
current deck
EDHREC signals
cached Archidekt/Moxfield/cEDH references
reference card frequencies
recurring card packages
Commander Spellbook combo evidence
AI Agent ADD/CUT evidence
Reddit posts + top comments
web-search results + selected page excerpts
Wizards rules page
discoverable Comprehensive Rules text
selected bracket
simulation objective
```

The model returns structured JSON containing:

```text
commander summary
primary archetypes
recommended archetype
game plan
candidate win packages
priority cards
cards to question
mulligan priorities
interaction priorities
mana observations
community findings
rules observations
evidence notes
```

No hidden chain-of-thought is requested or displayed.

### Rules hierarchy

The AI is advisory. It cannot override Commander Lab's deterministic systems.

Priority order:

```text
local Scryfall card/legality data
Commander deck legality / color identity
Commander Spellbook verified combo information
Commander Lab bracket detector
Commander Lab simulations
official Wizards rules evidence
reference deck evidence
community/Reddit opinions
local AI strategic synthesis
```

A model recommendation that is off-color, missing from the local card
database, Commander-illegal, or otherwise fails local validation is discarded.

### The AI now changes candidate construction

This is not only a chat summary.

The AI-recommended archetype is inserted into the Agent's candidate-theme
search. Additional AI-discovered build paths are also tested.

The model can additionally return priority cards. During AI Upgrade the
generator may promote up to eight of those cards into flexible slots, but only
after checking:

```text
card exists locally
Commander legal
inside commander color identity
not already present
not a land replacement
does not displace protected combo/theme pieces
has sufficient grounded shell support
```

Every resulting deck is then run through the normal bracket validation again.

Thus the AI proposes strategic experiments; Commander Lab decides whether they
are legal and whether simulations actually improve the objective.

### Research cache

AI web/community research is cached for 14 days under:

```text
data/ai_research_cache/
```

A research session can use Reddit search results, top Reddit comments, public
web-search snippets, selected public page excerpts and Wizards rules material.
Cached results are reused instead of repeatedly requesting the same hosts.

Existing Commander Lab reference/source caches remain separate and continue to
accumulate decklist evidence.

### AI Optimization loop

The AI Optimization now operates approximately as:

```text
deterministic commander research
        ↓
local AI research synthesis
        ↓
AI build paths + priority cards
        ↓
bracket-valid candidate generation
        ↓
guarded AI substitutions
        ↓
quick simulations
        ↓
objective scoring
        ↓
accept / reject
        ↓
test next build path
        ↓
full-intensity final simulation
        ↓
final bracket validation
```

For normal decks the objective remains earliest **reliable** win. For Stax the
objective remains earliest reliable lock, with win speed secondary.

The Agent screen shows the connected local model, recommended archetype,
research-source count, priority cards and leading win packages. Card names in
that display continue to feed the shared clickable card preview.


## 0.19.1 — Agent strategy interview + deck conversation

### The Agent asks what you are building before it tests

`RUN AGENT` / `AI OPTIMIZATION` no longer starts simulations immediately.

The Agent first opens a strategy interview:

```text
COMBO
CONTROL
MIDRANGE
VALUE / ENGINE
REANIMATOR
TRIBAL / KINDRED
STAX
BATTLECRUISER
CUSTOM
```

Each preset is a strategic preference, not an exclusion filter. For example,
choosing Tribal tells the research/optimization layer to preserve the deck's
kindred identity while still improving mana, interaction, packages and win
conditions.

`CUSTOM` accepts a free-form description such as:

```text
I want a graveyard-heavy artifact control deck that wins with compact combos
but still feels like a Shorikai vehicle/value deck.
```

That answer is passed into the local AI research context before the Agent
establishes its simulation baseline. It also becomes part of candidate-theme
selection and the final generated plan notes.

Selecting Stax automatically changes the objective to fastest reliable lock.
Selecting a non-Stax style returns the default optimization objective to
fastest reliable win unless the user changes it again in the Agent controls.

The interview supports keyboard and mouse selection.

### AI Agent CHAT

AI Agent now has ten tabs:

```text
OVERVIEW
AGENT
CHAT
RESEARCH
SIMULATE
SPEED
UPGRADE
RECOMMEND
SOURCES
TRACE
```

`CHAT` is a conversation window for the currently analyzed deck.

The conversation uses the same local Ollama connection as the research Agent
and is grounded with:

```text
commander Oracle/card data
current decklist
the user's chosen deck style
completed AI Agent evidence when available
current AI commander profile when available
recent conversation history
```

Useful questions include:

```text
Why did you recommend this combo?
What are the weakest cards in this build?
How would you make the mana more reliable?
What should I mulligan for?
Does this card actually fit the deck's plan?
What would you change if my pod has more graveyard hate?
Show me the difference between the combo and control build paths.
Why was this card cut after simulation?
```

The Agent chat is advisory. It cannot override Commander Lab's deterministic
legality, color identity, bracket, Commander Spellbook or simulation systems.

Chat runs in a worker thread so the TUI remains responsive while the local
model is answering.

Controls:

```text
type normally
Enter        send
Backspace    edit
Tab          next AI Agent tab
Shift+Tab    previous AI Agent tab
Esc          return to analysis
```

The CHAT page also has clickable `SEND MESSAGE` and `CLEAR CONVERSATION`
buttons.

Conversation state is cleared when a different deck is analyzed so discussion
from one deck cannot accidentally bleed into another.


## 0.19.3 — Ollama starts and stops with Commander Lab

For the normal local configuration, users no longer need to run:

```powershell
ollama serve
```

before launching Commander Lab.

Startup now performs:

```text
read data/ai_config.json
        ↓
local AI enabled?
        ↓
is 127.0.0.1:11434 already answering?
        ├─ yes → use existing Ollama; do not own it
        └─ no
             ↓
        find ollama executable
             ↓
        launch: ollama serve
             ↓
        wait for /api/tags
             ↓
        use local AI
```

On Windows Commander Lab checks PATH plus common locations such as:

```text
%LOCALAPPDATA%\Programs\Ollama\ollama.exe
%LOCALAPPDATA%\Ollama\ollama.exe
%ProgramFiles%\Ollama\ollama.exe
```

If Commander Lab launches the server, it owns that process for the session and
closes its process tree when Commander Lab exits.

If Ollama was already running before Commander Lab started, Commander Lab does
**not** close it. This avoids unexpectedly shutting down another application
that was using the same local server.

If Ollama is not installed, Commander Lab continues to work with its
deterministic AI Agent/optimization systems and reports the missing local AI
runtime in the status bar.

If `ai_config.json` points to a non-local Ollama/API-compatible server,
Commander Lab does not try to start or stop that external service.


## 0.19.4 — brief-driven Agent + complete local-AI bootstrap

### Agent controls are now intentionally minimal

The Agent screen now has one compact row:

```text
[ QUICK ] [ STANDARD ] [ THOROUGH ] [ EXHAUSTIVE ] [ GIVE/EDIT AGENT BRIEF ]
```

Everything strategic comes from the text brief.

A user can write:

```text
Bracket 4 Shorikai artifact combo-control. Keep Shorikai central to the deck.
I want reliable turn-5 wins rather than glass-cannon turn-3 hands. Avoid stax.
Test consistency and recovery after interaction. Explain every cut and tell me
which opening hands I should mulligan.
```

Commander Lab derives the target bracket and broad optimization objective from
that brief when those details are explicit. If no bracket is written, the
currently detected bracket is used.

The AI also extracts two explicit categories from the brief:

```text
testing requests
feedback preferences
```

Those instructions are fed into the Agent research/decision trace. They do not
cause Commander Lab to falsely claim unsupported simulations: deterministic
simulation results remain authoritative.

Thoroughness controls computational effort rather than strategy:

```text
Quick       1 optimization pass
Standard    3 optimization passes
Thorough    5 optimization passes
Exhaustive  8 optimization passes
```

The corresponding AI Agent simulation sample sizes remain controlled by the
existing TestingIntensity settings.

### Complete Ollama bootstrap on Windows

Commander Lab now checks all three local-AI dependencies at startup:

```text
1. Is Ollama installed?
2. Is the Ollama localhost API running?
3. Is the configured model installed?
```

If Ollama is missing on Windows, Commander Lab invokes Ollama's official
PowerShell installer:

```text
https://ollama.com/install.ps1
```

Installer output is shown before the TUI starts so installation errors are
visible.

Once Ollama is available Commander Lab starts/reuses the localhost server and
checks `data/ai_config.json` for the requested model. If that model is absent,
Commander Lab calls Ollama's `/api/pull` endpoint and displays download
percentage/status in the console.

Only after dependency setup finishes does the normal TUI open.

If Ollama or the model cannot be installed/downloaded, Commander Lab still
starts and falls back to its deterministic AI Optimization rather than
making the rest of the application unusable.


## 0.19.5 — user-supplied reference decks and pages

The automatic EDHREC, Moxfield, Archidekt, Commander Spellbook, cEDH and web
research remains active, but users can now explicitly tell Commander Lab which
decks or pages should guide a build.

The Sources screen includes:

```text
[ + ADD USER REFERENCE ] [ REMOVE SELECTED USER REFERENCE ]
```

A Moxfield or Archidekt deck URL is imported as a complete parsed decklist and
stored with the current deck's source sidecar. Other public URLs are fetched
and cached as textual strategic evidence.

User references are intentionally weighted above automatically-discovered
community examples, while remaining below deterministic legality, color
identity, bracket validation and verified combo/rules data.

References can also be supplied naturally inside the Agent brief:

```text
I want this deck to play like this list:
https://www.moxfield.com/decks/...

Keep its interaction density and compact win package, but stay in Bracket 4.
```

or directly in Agent Chat:

```text
Compare my deck to https://archidekt.com/decks/... and explain which engine
pieces make that list more consistent.
```

Commander Lab extracts URLs from the prose, imports/caches them, attaches them
to the current deck and makes them available to that Agent run or chat reply.

Imported reference decks are given to the local model with their commander,
decklist and source URL. Generic pages are supplied as cached excerpts. The
Agent is instructed to study gameplan, engines, card packages, mana structure,
interaction profile and win conditions rather than blindly copying cards.

A global cache is stored under:

```text
data/source_cache/user_references/
```

so reusing the same reference URL for another deck does not repeatedly request
the source website.


## 0.19.8 — scrollable text panes and per-deck history

Long text areas are no longer first-page-only views.

The following panes support scrolling:

```text
Dashboard Sources
Dashboard Log
AI Optimization
AI Agent Chat history
AI Agent Sources
AI Agent Trace
Agent build-brief textbox (auto-scroll while typing)
Agent Chat input textbox (auto-scroll while typing)
```

Sources, Log and Trace also use mouse-hover highlighting. Moving the mouse over
a visible source selects/highlights it without opening it; Log/Trace highlight
the visible line under the pointer.

The Agent build brief shows:

```text
TYPE YOUR DECK GOALS HERE • LINE 4 COL 17 • 312 CHARS

...your text here▌
```

so it is clear where typing is occurring. Chat input exposes the same line,
column and character information.

### Deck-local test history

Each deck now receives a history sidecar beside the deck file:

```text
decks/
  My Deck.txt
  My Deck.sources.json
  My Deck.history.json
```

`My Deck.history.json` stores the latest 80 cached runs, including:

```text
draw simulations
goldfish tests
Commander Spellbook combo checks
AI Agent research
AI Agent simulation
AI Optimization optimization
```

A run contains its timestamp, category, result summary and relevant
trace/sample-log lines.

Opening a deck automatically loads its history. The normal deck-analysis tabs
now include:

```text
LOG
```

which provides a scrollable view of that deck's previous testing and Agent
work. This history is separate from the global diagnostic application log:
the deck history is intended to answer "what testing have we already done on
this exact deck?"


## 0.19.9 — faster pasted deck import

`PASTE DECKLIST` now opens directly into the decklist window:

```text
PASTE DECKLIST
┌─────────────────────────────────────────────┐
│ 1 Sol Ring                                  │
│ 1 Arcane Signet                             │
│ ...                                         │
└─────────────────────────────────────────────┘
Mouse wheel / ↑↓ / PgUp PgDn scroll
F2 or Ctrl+Enter continue
```

The view only renders the currently visible lines, so a large pasted list does
not make the TUI repeatedly build a giant text widget.

After parsing:

```text
Commander detected?
    │
    ├─ YES → ask for deck name
    │
    └─ NO  → ask user for commander
                  ↓
             resolve/canonicalize
                  ↓
             ask for deck name
```

At the final naming prompt, pressing Enter with an empty field uses the
commander name automatically. Partner commanders use both names joined with
` + `.


## 0.20.1 — Chat is the steering wheel for the Agent

The Chat screen now controls optimization directly:

```text
[ QUICK ] [ STANDARD ] [ THOROUGH ] [ EXHAUSTIVE ]
[ RUN AGENT WITH CHAT DIRECTION ]
```

A user can talk normally:

```text
I like the current build, but it is relying too heavily on the commander.

Keep the same win package, improve the backup engines, and compare it to:
https://www.moxfield.com/decks/...
```

Then click `RUN AGENT WITH CHAT DIRECTION`.

Commander Lab combines the base build brief with the recent user-side chat
instructions and uses that combined direction for the next research,
generation and simulation pass. Text currently sitting unsent in the message
box is also included when the Run button is clicked.

While the Agent runs, Chat remains visible and shows the Agent progress bar and
current task. When the run finishes, the result is added to the conversation,
so the next message can be a correction such as:

```text
That got faster, but I don't like losing the graveyard package.
Put that package back and find cuts elsewhere.
```

and the Agent can be rerun immediately.

### Goldfish UI removal

The standalone Goldfish tab/manual playtest window has been removed from the
user interface. Commander Lab still uses its simulation engine internally for
AI Agent and Agent testing, including earliest/likely win-turn statistics.
