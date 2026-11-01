# Changelog

## 0.32.1

- **Fixed: card images overlapping on phones.** A card with a long name, usually a double-faced card like "Bloodline Keeper // Lord of Lineage", widened its column so its image spilled over the next card. Card grids now keep every column the same width whatever the name, in Suggestions, Search, the Cards tab's image view and everywhere else that lays cards out in columns.

## 0.32.0

- **Budgets for generated decks.** The Build screen has a **Budget**: a total for the whole deck ($50, $100, $250, $500 or your own amount), a per-card limit, or both, in US dollars from Scryfall.
  - Cards over the limit never enter the build. With a total, the per-card ceiling is worked out from what's left after the commander's price.
  - If the finished deck is still over, its most expensive cards are swapped for cheaper ones that fill the same role at a similar mana value. Expensive lands become cheap lands in your colors, or basics. The commander and its combo pieces stay.
  - The deck's notes list each swap and the final price, and say so if the commander alone puts the total out of reach.
  - Works for Commander, Pauper Commander and Canadian Highlander (where swaps never add pointed cards).
- **Suggestions can go to the sideboard or Considering.** Each suggestion has **+ Sideboard** and **+ Considering** next to **Add**, and shows when a card is already in one of them.

## 0.31.0

- **Every copy of Commander Lab updates itself from Settings.**
  - **Android:** **Download and install** downloads the new APK inside the app and opens Android's installer, which updates the app in place and keeps your decks. The first time, Android asks you to allow Commander Lab to install apps; once you turn that on and come back, the installer opens by itself.
  - **Built from source** (`./run-linux.sh`): Settings compares your checkout with GitHub and shows what's new. **Update and rebuild** pulls the code, rebuilds with progress shown, and restarts with the same options.
  - **Linux `.deb`:** downloads the new package and installs it after your system asks for your password, then restarts. Without a password prompt available, it opens the package in your software installer.
  - **Browser-edition binary** run on its own: replaces itself and restarts.
  - Windows, the Linux `.sh` install and AppImages update as before.

## 0.30.2

- **Linux ships the desktop app.** The `.sh` opens the desktop app on every distro, NixOS included, and adds **Commander Lab** to your app launcher. On NixOS it runs through `appimage-run`, and gets that from Nix for the run if it isn't installed. The browser edition is only used with `--web`, or when the desktop app can't start.
- **Building from source:** `./run-linux.sh` builds the desktop app by default (`--web` for the browser edition). On NixOS it builds inside `shell.nix`, which now includes the GTK settings files the desktop app needs. If the WebKitGTK libraries are missing, it lists the packages to install.
- **Fixed:** a desktop app built from source with `cargo` opened a blank window. It was a development build looking for a dev server; the script now builds the release version with the interface built in.
- The window no longer comes up blank on NVIDIA and some Wayland setups: the Linux launchers set `WEBKIT_DISABLE_DMABUF_RENDERER=1`.
- The desktop window opens centered at a size that fits smaller laptop screens.

## 0.30.1

- **Fixed: new builds didn't show up as updates.** The app reused its last update check for up to six hours and only checked when it started, so a build published after that check stayed hidden. Now:
  - opening **Settings** always checks GitHub;
  - the app checks again every hour while it's open, and when you come back to it after a while (useful on Android, which keeps apps running for days);
  - a quick relaunch reuses the last answer for at most ten minutes.
- If the update check fails, Settings now says why instead of quietly saying you're up to date. A **Releases** link there opens the GitHub downloads page.

## 0.30.0

- **Search tab: find cards with Scryfall syntax and add them, like Moxfield.**
  - Type any Scryfall search (`t:creature o:flying mv<=2`, `otag:removal`, `usd<1`). **Only cards that fit this deck** adds your colors and format for you.
  - Sort by popularity, name, mana value, price, release date, rarity, color or power. Show one result per card, per art or every printing.
  - Quick filters for creatures, instants, ramp, card draw, removal, board wipes, tutors, counterspells and cheap cards.
  - Add each result to the **Deck**, the **Sideboard** or **Considering** with one tap. A badge shows how many you already have. Adding from **Every printing** adds that printing.
  - When a search has a mistake, Scryfall's explanation is shown.
- **Considering (maybeboard).** A new section for cards you're thinking about. It's kept with the deck but isn't counted in its size, price or legality. Move cards between the deck, sideboard and Considering from the card menu. Imported "Maybeboard" sections land here.
- **Combos show card images.** Each combo shows its cards. Missing pieces are outlined and marked **Add this**; tap a card to see it and add it.

## 0.29.0

- **Pauper Commander uses Pauper Commander sources.** Card choices come from **PDHREC** (pdhrec.com), the PDH counterpart of EDHREC: how often each card shows up across a commander's decks, and its synergy. Public **Archidekt decks in the Pauper EDH format** feed the reference decklists. EDHREC and Commander decklists are no longer used for PDH, and the Suggestions tab reads PDHREC for PDH decks too.
- **Canadian Highlander uses Canadian Highlander decklists.** The builder reads tournament lists from **MTGTop8**'s Canadian Highlander section (by archetype) and the most viewed **Moxfield** Highlander decks. It keeps the lists in your colors and builds from how often cards appear in them, weighting exact color matches and decks of your play style highest. When only a few lists match your colors, card quality and the deck's needs carry more of the choice, and the notes say so.
- The Sources tab lists the sources each format actually used (PDHREC and Archidekt for PDH; MTGTop8 and Moxfield lists for Highlander).
- **Fixed:** Scryfall's card file includes tokens and emblems with the same names as real cards ("Ornithopter", "Treasure"); a lookup could pick the token. They're no longer imported, and older card data is cleaned up on start.
- **Fixed:** the check for which cards can lead a PDH deck now paces itself under Scryfall's rate limit, retries when asked to slow down, and keeps partial results. If it still couldn't finish, picking a commander confirms it with Scryfall on the spot instead of rejecting it.

## 0.28.0

- **Updates inside the app.** Commander Lab checks GitHub for a newer build when it starts. When one is out, an **Update** button appears (in the sidebar on desktop, a banner, and a dot on Settings), and **Settings → Updates** can check any time.
  - Windows installer: downloads and installs the update, then reopens the app.
  - Windows portable exe and Linux (the .sh install or an AppImage, including NixOS' browser edition): replaces itself and restarts.
  - Android: downloads the new APK; Android asks to install it over the current app and keeps your decks.
  - What's new is shown from the changelog before you update.
- **Export** (replaces "Export to Moxfield"):
  - **Moxfield** and **Archidekt**: copies the list in that site's import format (printings kept; the commander lands in Archidekt's Commander category) and opens the site to paste it. Neither site lets other apps create decks in your account, so this is the most direct route.
  - **Tabletop Simulator**: a saved deck object with every card's image, the commander face up beside the deck, and double-faced cards as two-state cards. On a computer with TTS installed it's saved straight into TTS' Saved Objects; otherwise download the file and put it there.
- **Commander search** no longer stays blank when something's wrong. It says why: for example, "Update card data to search Pauper Commander commanders" when the card data predates 0.27, or that no eligible commander matches. Picking a result from the list is more reliable on touch screens.
- A prompt to update the card data appears when it's too old for Pauper Commander and Canadian Highlander.
- Links to Scryfall, TCGplayer, mpcfill.com and others open in your browser from the desktop and Android apps.
- **Run from source on Linux:** `./run-linux.sh` builds and starts the browser edition from a git checkout (`--desktop` for the window, `--update` to pull and rebuild). On NixOS it uses the included `shell.nix` automatically.

## 0.27.2

- **Proxies: every art choice.**
  - Each card used to show at most 12 MPC Autofill images. Now the first 40 load right away and **Load more** brings in the rest.
  - The default 300 DPI minimum no longer hides scans. It's now off; set it under Order options if you want one.
  - Cards that find nothing are searched again with other spellings and fuzzy matching. Split and adventure cards ("Fire // Ice") are searched by their full name, both halves, and the first half.
- **Proxies: official art.**
  - Every distinct official artwork of each card from Scryfall is offered next to the community scans: one per artwork across all paper printings, both faces of double-faced cards, marked **Official** with the set.
  - Filter the art by All / Official / MPC. Turn on **Official art first** or turn official art off in Order options.
- When you export, chosen official scans are downloaded next to the order and the XML points at those files (the MPC Autofill desktop tool v4+ accepts local files).
- If the MPC Autofill server can't be reached, the search still offers official art and says why.

## 0.27.1

- Card images are fitted, never cropped. Battles, planes, split cards and proxy art with bleed used to lose their edges or look zoomed in.
- Game: a **Leave table** button at the top of the table on desktop (and in the game menu everywhere), and Esc leaves on desktop. The game is kept; Resume brings you back.

## 0.27.0

- **Pauper Commander (PDH):** build, check and track PDH decks under the PDH Home Base rules.
  - The commander is a creature, Vehicle or Spacecraft printed at uncommon (an uncommon Background can be the second commander).
  - The 99 are cards printed at common. Only paper and MTGO printings count; Arena doesn't.
  - Mystic Remora, Rhystic Study, ante cards, Conspiracies and the cards Wizards removed from constructed play are banned.
  - Games are played at 30 life with 16 commander damage.
  - Commander search on the Build screen only offers eligible commanders.
- **Canadian Highlander:** pick colors and play styles instead of a commander.
  - Builds 100 singleton cards with no sideboard, using the Vintage card pool and banned list.
  - Stays within the 10-point limit (points list effective 1 September 2026). The power level sets how many points it may spend.
  - Every card is chosen from its rules text, so it doesn't depend on Commander decklists.
- **Deep card search** (an option when generating): reads the rules text of every card in Scryfall's database for cards that fit your play styles, including older cards no decklist runs.
  - Example: wither and persist creatures for a -1/-1 counters deck.
  - The deck notes list what it found and why.
  - Rarely played finds are capped so the deck keeps a proven core.
- **Your own play styles:** type any ability or theme on the Build screen (for example "-1/-1 Counters", "Elves" or "Landfall") and the deck is built to match it.
- Theme matching is smarter everywhere: creature types, plurals, old wordings, and +1/+1 versus -1/-1 counters are told apart.
- **Suggestions:** a "Found by rules text" group, plus a search box to dig for cards with any ability you name. Suggestions only offer cards legal in the deck's format.
- **Deck page:** shows the deck's format, a points total for Highlander decks, and format-specific legality problems. Change a deck's format from the ⋯ menu.
- **Game tab:** choose the game mode. Each sets starting life, commander damage and player count:
  - Commander
  - Pauper Commander
  - PDH one-on-one
  - Canadian Highlander
  - Brawl
  - Oathbreaker
  - 60-card
  Modes without commander damage hide it, and the commander-tax counter, entirely.
- Card data now records rarity and Pauper Commander / Vintage legality. **Update card data once** (Settings) to use the new formats.
- Sticker sheets and Attractions are never put in a deck.

## 0.26.0

- Game: choose which way the life panels face — **Across** (top row faces the players opposite), **Sides** (the left and right columns face the long edges, like Mythic Tools) or **All me**. Pick it before the game or change it from the game menu. Each player can also turn their own panel a quarter turn from their menu, and the menu, set-life box and swipes follow that panel's direction.
- Game: tap a counter on a player's panel (poison, tax, energy, custom counters, or the ⚔ commander-damage chip) to edit it right there with − / +, Clear and Done, turned to face that player.
- Game: tap the ♛ Monarch or ⚔ Initiative token to choose who gets it (or nobody). Dragging it still works.
- Game: 6-player panels turned sideways keep a big life total; the name, tokens and counters float along the edges.

## 0.25.0

- Sharing: the QR now holds only what rebuilds the deck (commander, cards and counts) in a denser QR-native encoding. A typical 100-card deck went from QR size 31 to 22, much easier to scan. "Include printings and sideboard" adds them back. Older share codes still import.
- Fixed: opening Share or a card preview no longer zooms the whole app out on Android. The app no longer pinch-zooms like a web page.
- cEDH generation: at Bracket 5, cEDH Decklist Database lists are now the primary evidence for card choices; casual decklists and EDHREC only break ties. The deck notes say which sources drove the build, and fall back clearly when a commander has no cEDH lists.

## 0.24.0

- Game tab reworked to play like Mythic Tools:
  - Tap a side for ±1, hold for ±10; set an exact life total (with ±1/5/10/100) from the player menu.
  - Swipe up or down on a panel (or tap ⋯) for that player's menu.
  - "Deal commander damage" turns every other panel into a damage counter for that commander; one tap per point, "Done with combat" to finish.
  - Drag the Monarch or Initiative token from one player to another.
  - Card-art backgrounds from any card, low-life pulse at 10 or less, concede.
  - Turn tracking in clockwise seat order with an optional chess clock (10–45 min per player).
  - Life log of every change, floating mana pool, in-game card lookup, dice/coin/first player.
  - End a game by choosing the winner; saved games show win rates per player and deck.
- Deck page: the ⋯ menu (rename, duplicate, delete, edit as text) is no longer hidden behind the tabs.
- Android: content no longer sits under the status bar.

## 0.23.0

- New Game tab: life totals for 2–6 players on one phone, with the top row turned to face players across the table. Tap or hold to change life; per-player commander damage from each opponent (partners tracked separately), poison, commander tax, energy, experience, rad, tickets and custom counters; Monarch, Initiative, City's blessing and day/night; dice, coin flip, random first player, undo, lethal detection, and the game is kept if you close the app. Link a deck to show its commander art.
- Build: pick up to three play styles; the first is the main focus and the others are blended in.
- Suggestions: one mixed grid with every card the same size, tagged with why it's suggested (combo, synergy, popular, fills a gap), plus a filter by reason.
- Proxies: searches now go to mpcfill.com, the current MPC Autofill server (the old address returned "405 Method Not Allowed"); saved settings move over automatically, and cardbacks load correctly.
- Phones: checked at 360–412px wide and landscape; larger tap targets, safe-area padding for status bars.

## 0.22.0 — the graphical app

Commander Lab is now a graphical app for Windows, Linux and Android instead of a terminal program. The deck engine is unchanged underneath and shared by every platform.

- New interface: deck library with commander art, Moxfield-style deck editor (search-to-add, +/−, move to sideboard, make commander, list or image view), card details with store links.
- Card printings: choose any printing of any card with art and prices, and mark cards foil. Printings are kept in the decklist (`1 Sol Ring (C21) 263 *F*`) and used by Moxfield export.
- Suggestions tab: cards to add as images, grouped by combo completions, EDHREC synergy, popularity and role gaps, with role and price filters.
- Version history: every change is recorded with a diff; view or restore any version without losing the current one.
- Share by QR code or share code (deck + printings, no server); scan with the camera or a photo, or paste the code.
- "How to play" guide built from the deck's cards: strengths, weaknesses, mulligans, early turns, engines, win lines and interaction.
- Analysis: goldfish win-speed test and "rebuild at another bracket" alongside the existing curve, colors, roles, bracket, legality, cuts/adds and draw simulation.
- Combos, Sources (reference decks/pages), Proxies (MPC Autofill) and AI Agent (runs, ranked results, save as deck, chat) are now full screens.
- Settings: dark/light/system theme, currency, card data updates, AI provider and model.
- Builds: Windows installer and portable exe, Linux self-installing `.sh` (desktop window, or browser edition on NixOS), AppImage, `.deb`, static browser edition, and Android APK (all signed with the same key so updates install over each other).
- Removed: the terminal interface.

## 0.21.0

- New MPC tab: MPC Autofill search, per-card art cycling with live preview, double-faced back slots, foil/stock/sideboard options, XML order + mpcfill.com text export.
- Moxfield export (`M`) with `(SET) collector#` printings; printings now recorded during Scryfall UPDATE.
- Deck integrity gate after generation and AI upgrade: size, singleton, identity, legality, commander placement auto-repair; bracket re-lock; land/role/goldfish warnings.
- Local AI backend is optional at startup (`COMMANDER_LAB_NO_AI=1` to skip).
- Clipboard falls back cleanly where unavailable (Android, headless Linux).
- GitHub Actions builds Windows `.exe`, Linux self-installing `.sh` (static musl), and Android `.apk` on every push and publishes a release.
- Test fixtures are optional so clean checkouts pass `cargo test`.

## 0.4.0

- Added daily Scryfall price fields and marketplace purchase URIs.
- Added shared clickable card-preview behavior to analysis areas containing card names.
- Added storefront launch buttons.
- Added archetype/theme detector and archetype-aware cut/add scoring.
- Added Commander bracket estimate and Commander Spellbook bracket refinement.
- Added GOLDFISH tab and local heuristic goldfish simulation.
- Added sideboard preservation/browsing.
- Added main-menu REFRESH and NEW DECK actions.
- Added pasted deck creation, Moxfield URL import, and optimized local deck generation.
- Added bracketed-paste support.
- Retained JSONL.GZ Scryfall streaming importer.
- Retained all UI RGB values in `src/theme.rs`.


## 0.4.1

- Fixed Rust `E0502` in `render_modal` by cloning modal state before registering clickable regions.
- Removed the unused `mana` assignment warning in the goldfish draw-spell branch.


## 0.5.0

- Interactive goldfish/playtest sandbox.
- Clickable archetype tag drill-down.
- Shared card-art preview from playtest, archetype and combo areas.
- Separate SIM tab retained.
- Persistent runtime theme editor.


## 0.5.1

- Fixed Rust E0382 in `analyze_selected`.
- `PlaytestState` is now built while `parsed` is still locally owned, before
  `parsed` is moved into `self.parsed`.


## 0.5.2

- Removed THEME from deck-analysis tabs.
- THEME is now a dedicated top-level screen opened from the main deck library only.
- Theme swatches remain clickable on the standalone screen.
- Esc or BACK returns to the main deck library.

## 0.6.0

- Partner-aware generation.
- Combo/game-plan deck generator.
- CUT/ADD sideboard action.

## 0.6.1

- Bracket 1–5 selection for generated decks.
- Bracket-aware combo/tutor/fast-mana/Game-Changer weighting.
- Main-library DELETE button with confirmation and plan-sidecar cleanup.


## 0.7.0

- Total deck and sideboard price.
- Archidekt import.
- Cached public Moxfield/Archidekt deck references.
- cEDH DDB reference layer for Bracket 5.
- RGB theme picker.
- Clipboard decklist export.


## 0.7.1

- Cleaned the 0.7.0 dead-code warnings without hiding them with blanket allows.
- Price tile now includes known-card and unpriced-card counts.
- Bracket screen now reports extra-turn and mass-land-denial counts.
- Goldfish reference panel now displays all stored simulation metrics,
  sample opening hand/actions, combo lines checked, and simulation caveat.
- NEW HAND now uses the existing `PlaytestState::reset` method.
- Removed obsolete cached-bulk rebuild, reference-cache wrapper, hex theme
  parser, and superseded detail-list renderer.


## 0.8.0

- Fuzzy commander autocomplete/correction and assumption handling.
- Partner detection runs after corrected canonical names are resolved.
- Added pre-generation commander-theme selection.
- Added live asynchronous generation progress screen.
- Tightened flexible-slot eligibility to theme/role/reference/combo-backed cards.
- Theme-compatible/reference-supported combo seeding replaces generic color-only combo stuffing.
- Game-plan output records direct theme density.


## 0.8.1

- Added cached EDHREC commander-theme retrieval.
- EDHREC themes now outrank local inferred themes during generation.
- Increased generation theme choices from 6 to 12.
- Theme picker now uses two clickable columns.
- Local inference remains as a fallback for new/unavailable EDHREC data.


## 0.8.2

- Fixed commander tag extraction to read `panels.taglinks`.
- Sorts commander tags by EDHREC deck count.
- Uses the actual EDHREC top 10 for generation theme choices.
- Local inference is supplemental only.


## 0.8.3

- Theme selection is now a soft play-style preference.
- Removed hard off-theme card eligibility checks.
- Off-theme cards can win slots on overall deckbuilding score.
- Combo packages fall through from theme matches to reference matches to all
  other bracket-appropriate legal packages.
- Final deck fill scores all remaining legal cards.


## 0.9.0

- Added commander-specific EDHREC card-list intelligence.
- Top EDHREC commander tags now define a persistent commander core.
- Added commander rules-text engine-term detection.
- Added engine-density and EDHREC-core-density scoring.
- EDHREC high-synergy cards can enter candidate pool outside global top-15k.
- Removed arbitrary color-only combo seeding.
- Spellbook completion/reporting now requires build relevance.
- Mana-base ranking now uses commander-specific EDHREC/reference signals.


## 0.10.0

- Added keyboard focus/navigation to CUT / ADD, SIM, and COMBOS.
- Added SOURCES tab with clickable exact deck-generation/reference URLs.
- Generated decks save `.sources.json` alongside `.txt` and `.plan.md`.
- Added AI AGENT local web-research/recommendation tab.
- AI Agent combines EDHREC, Moxfield, Archidekt, cEDH DDB, Scryfall and
  current-list signals without requiring a paid AI API.
- AI Agent ranks missing commander-core cards and low-support current cards.


## 0.10.1

- Fixed E0425 caused by an accidental `selected` reference inside `render_tile`.
- `render_tile` once again uses its supplied border color normally.
- `render_pct_gauge` now correctly uses its `selected: bool` argument to show
  keyboard focus on SIM metrics.


## 0.11.0

- Replaced Metro default palette with Taskii-inspired warm brown/amber palette.
- Added automatic migration only for the exact old built-in Metro theme.
- Reworked main deck library into a wide three-column terminal layout.
- Reworked deck analysis into left metadata / center active tab / right notes.
- Moved analysis tabs to the lower edge of the workspace.
- Restyled buttons as dark outlined terminal controls.
- Added compact fallback layout for narrower terminals.


## 0.12.0

- Added shared `CommanderIntelligence` model.
- Generator and AI Agent now use the same learned commander shell.
- Merges base EDHREC shell with selected-theme EDHREC shell.
- Learns ramp/draw/interaction/tutor/wipe/protection/recursion targets.
- Added confidence-weighted per-card support scoring.
- Combo seeding and Spellbook completion use learned combo relevance.
- AI Agent exposes model confidence and per-card confidence.


## 0.13.0

- Added explainable AI Agent decision trace.
- Added selectable Quick/Standard/Thorough/Exhaustive simulation intensity.
- Added exact T1–T8 known-combo win-turn distribution and earliest/likely/median win turn.
- Added Commander Spellbook combo variant hover/keyboard explorer and source links.
- Added named theme library: Lambda, Dracula, Gruvbox Dark, Nord, Tokyo Night,
  Solarized Dark, and persistent user themes.
- Added generation BUILD DECISION TRACE.


## 0.14.0

- Added Neon Slate as the new default theme.
- Preserved the previous warm theme as Lambda.
- Changed theme editor labels from implementation colors to semantic UI roles.
- Rebuilt wide deck-library layout around top cards, main list/detail pane,
  activity panel, quick actions, and bottom key strip.
- Rebuilt wide analysis layout around the same application shell.
- Added compact top status/header and clock.


## 0.14.2

- Main menu top row reduced from five cards to three:
  - Deck Library
  - Cards / Status
  - AI Agent
- Removed the main-menu Sources summary card.
- Combined card database count and database readiness into Cards / Status.
- Restored click hitboxes for wide-layout deck rows.
- Made main-menu summary cards clickable.
- Made selected-deck Analyze and AI Agent controls clickable.
- Rebuilt main-menu Quick Actions with real button hitboxes.
- Made dashboard Cards, Bracket, Combos, and AI Agent summary cards clickable.
- Rebuilt dashboard Quick Actions with real clickable buttons.


## 0.15.0

- AI Agent is now its own application Screen.
- Removed AI Agent from the normal Dashboard tabs.
- Added Overview / Research / Simulate / Win Speed / Upgrade /
  Recommendations / Sources / Trace tabs.
- Separated commander research from game simulation.
- Added exact T1-T8 match-count win-speed breakdown.
- Added known winning-line frequency breakdown.
- Added bracket-selectable AI Upgrade from the current deck.
- Added Copy Current Decklist inside the Upgrade workspace.


## 0.15.1

- Fixed AI Upgrade generation progress type mismatch (`u16`).
- Fixed mutable/immutable borrow conflicts in dashboard summary cards.
- Precomputed dashboard display values before registering clickable regions.
- Removed dead private functions that had no remaining call sites:
  generator.rs::tutor_target, generator.rs::template_fits_tags, goldfish.rs::combo_complete, ui.rs::render_dashboard_left_sidebar, ui.rs::render_dashboard_right_sidebar, ui.rs::render_keyboard_named_card_list, ui.rs::render_footer_picker, ui.rs::render_footer_dashboard.

- Removed remaining unreferenced legacy UI helpers and unused PlaytestState::reset.


## 0.16.0

- Increased reference corpus to as many as 60 matching public decks.
- Increased Archidekt and Moxfield commander reference collection.
- Added larger cEDH DDB reference layer for Bracket 5.
- Added cached recurring card-pair/package analysis.
- Added Commander Spellbook combo scanning across up to 36/50 reference decks.
- Added reference combo frequency and recurring-combo ranking.
- AI Agent RESEARCH now shows common cards, recurring packages, and combos
  actually used by reference lists.
- Generator now pulls high-frequency reference cards into its candidate pool.
- Generator prioritizes recurring reference combos over generic fallback combos.
- Added competitive one-card-away combo completion pass for Bracket 5.
- Requested bracket is now post-build validated as a hard requirement.
- Added under-target power correction and over-target power-down pass.
- AI Upgrade revalidates the bracket after retaining cards from the original.
- Removed newly introduced unused support fields/helpers.


## 0.16.1

- Added persistent UI edge-style preference.
- Theme screen now includes a clickable `EDGES: ROUNDED` / `EDGES: SQUARE` control.
- Rounded mode uses Ratatui `BorderType::Rounded`.
- Square mode uses Ratatui `BorderType::Plain`.
- The selected edge style is applied across bordered UI panels, buttons, modals, lists, charts, and theme controls.
- Edge preference is saved separately to `data/ui_style.json`, so changing a color theme does not overwrite it.


## 0.17.0

- Added dedicated AI Agent `AGENT` workspace tab.
- Added local AI Optimization optimization agent; no paid LLM API required.
- Agent automatically runs research, baseline testing, candidate generation,
  candidate testing, objective comparison, iteration, final testing, and
  bracket revalidation.
- Added FASTEST WIN optimization objective.
- Added STAX LOCK optimization objective.
- Selecting a Stax archetype automatically switches the agent to lock mode.
- Agent tries the user's selected archetype first, then leading commander
  themes for fast-win optimization.
- Added B1-B5 target buttons directly to the Agent page.
- Added Quick / Standard / Thorough / Exhaustive controls directly to Agent.
- Added configurable 1-8 search rounds.
- Agent will not replace an already-correct-bracket current deck with a tested
  candidate that scores worse.
- Added heuristic Stax lock simulation with commander-zone lock pieces,
  stacked pressure, lock-turn distribution, and lock-piece frequency.
- Added Stax-specific SPEED report.
- Increased Stax theme density at higher brackets.
- AI Upgrade now protects generated theme-engine cards while retaining current
  cards, preventing Stax lock pieces from being accidentally displaced.
- Added universal card-art / card-data / price / storefront preview column to
  every AI Agent page.
- Added hover/click preview hitboxes for reference cards, recurring packages,
  reference combos, simulated win lines, agent changes, and Stax lock pieces.
- Agent work is included in AI Agent TRACE.


## 0.17.1

- Fixed commander eligibility detection for cards whose type line contains
  `Legendary` and `Creature` with another card type between them, including
  `Legendary Artifact Creature` and `Legendary Enchantment Creature`.
- Fixes commanders such as Urtet, Remnant of Memnarch.
- Explicit noncreature commanders such as Shorikai, Genesis Engine continue to
  qualify through their "can be your commander" Oracle text.
- Commander eligibility is now recomputed when cards are read from SQLite, so
  users do not need to rebuild an older Scryfall database just to receive the
  fix.
- Commander search no longer filters candidates using only the stale persisted
  `commander_eligible` bit.
- Live commander autocomplete now uses the same fuzzy resolver as generation.
- Added high-confidence leading-name resolution for inputs such as `Shorikai`,
  `Urtet`, `Muldrotha`, etc.
- Added punctuation-insensitive compact-name comparison.
- Added regression tests for artifact/enchantment creatures, explicit
  noncreature commanders, punctuation, short names, and small misspellings.


## 0.17.2

- Added persistent rotating `data/logs/commander-lab.log`.
- Added timestamped subsystem-scoped logging.
- Increased in-memory AI Agent / Agent / Generation trace retention to 500
  events per area.
- Added detailed AI Agent commander/model/reference/ranking logs.
- Added detailed simulation resource, win-speed and winning-line logs.
- Added detailed Agent baseline, candidate score, card-diff, acceptance and
  final-validation logs.
- Added reference cache/source/Spellbook corpus logging.
- Added detailed deck-generator candidate pool, combo package, role and final
  bracket logs.
- Added persistent bracket-correction swap logs.
- Generation and AI Agent TRACE views now show the persistent log path.


## 0.18.0

- Added persistent per-reference-deck cache for Moxfield, Archidekt and
  cEDH-linked decks.
- Added canonical URL cache keys to avoid duplicate `www`/trailing-slash
  source entries.
- Added 8-hour search/discovery page cache with stale-on-host-failure fallback.
- Added commander-wide persistent reference archives shared across brackets.
- Commander archives retain up to 500 unique decks; active analysis uses up to
  120.
- Existing 0.17.x reference snapshots seed the new cache automatically.
- Added crawl-state pagination so subsequent refreshes discover later result
  pages rather than repeatedly scraping only the first page.
- Limited each source to at most 12 newly downloaded decklists per refresh.
- Cached reference decks are reused without another host request.
- Added per-deck Commander Spellbook scan-complete state, including successful
  zero-combo results.
- Spellbook scans are now incremental and only spend budget on unscanned decks.
- Extended EDHREC JSON cache to 72 hours and added stale-on-error fallback.
- Added source-cache hit/miss/stale/budget/archive logging.
- Commander generation autocomplete now waits 300 ms after typing stops before
  running the fuzzy commander resolver.
- Autocomplete results are cached until the textbox changes.


## 0.18.1

- Commander autocomplete suggestions in CREATE DECK → GENERATE are now mouse
  selectable.
- Each visible suggestion row registers a dedicated hitbox.
- Clicking a commander suggestion fills the current commander-name segment
  without immediately advancing to theme selection.
- Partner/background workflows are preserved: select the first commander,
  type ` + `, pause for autocomplete, then click the second commander.
- Clicking a suggestion resets the autocomplete debounce/cache cleanly.
- Commander text editing explicitly invalidates cached autocomplete results.


## 0.18.2

- Reworked commander detection around one runtime `effective_commander_eligible`
  rule instead of trusting the persisted SQLite eligibility bit.
- Commander resolution, fuzzy suggestions, validation and partner selection now
  all use the same effective commander rule.
- Added compatibility handling for Shorikai, Genesis Engine when older/local
  card data omits or mangles the explicit commander-permission Oracle sentence.
- Added the same compatibility handling for Grist, the Hunger Tide, whose
  commander legality is a rules/text edge case for simplistic type checks.
- Existing SQLite databases are repaired on open for recognized legendary
  creatures, explicit commander-permission Oracle text and known compatibility
  exceptions.
- Added COMMANDER diagnostic logging showing exact lookup result, type line,
  Commander legality, Oracle-text length, eligibility rule/reason, fuzzy
  candidate and validation outcome.
- Commander errors now include the local type line, Oracle text length and the
  exact eligibility rule that failed.
- Added regression tests for Shorikai and Grist compatibility handling.


## 0.18.3

- Removed generated `<deck>.build.log.txt` files from the deck directory.
- Generation diagnostics are now UI-only at the App event/trace layer.
- Generation trace remains visible directly beneath the progress bar while the
  generator is running.
- Generation activity panel now behaves as a live tail and always shows the
  newest messages that fit in the visible area.
- Removed the persistent-log path from the generation modal.
- AI Agent/Agent persistent diagnostics remain available separately; this
  change specifically stops deck generation from creating per-deck log files.


## 0.19.0

- Added local Ollama AI integration with automatic model discovery/fallback.
- Added generated `data/ai_config.json` configuration.
- Added `ai_config.example.json`.
- Added structured AI Commander Profile.
- AI profile learns commander archetypes, gameplan, win packages, card
  priorities, mulligans, interaction/mana concerns and community findings.
- Added 14-day AI research cache.
- Added cached Reddit search and top-comment research.
- Added public web search and selected page-excerpt research.
- Added Wizards rules landing-page and discoverable Comprehensive Rules
  research.
- Added strict prompt guardrails: the local AI cannot override deterministic
  legality, bracket, Spellbook or simulation evidence.
- Added AI-recommended archetypes to the Agent's candidate search.
- Added guarded AI priority-card substitutions during AI Upgrade.
- AI card suggestions are filtered through local card existence, Commander
  legality and commander color identity.
- Completed AI-modified decks are bracket-validated again.
- Added local AI status/model/archetype/source count to Agent UI.
- Added AI commander summary, gameplan, priority cards and win packages to
  Agent results.
- AI research sources are merged into AI Agent/generated-deck provenance.
- Existing deterministic Agent remains the automatic fallback when Ollama is
  unavailable.


## 0.19.1

- Added mandatory Agent strategy interview before optimization begins.
- Added Combo, Control, Midrange, Value/Engine, Reanimator, Tribal/Kindred,
  Stax, Battlecruiser and Custom style choices.
- Added mouse-selectable strategy choices and clickable start control.
- Added free-form custom style input/paste support.
- User style is injected into local-AI research before baseline simulations.
- User style influences candidate build-path search without becoming a hard
  card-exclusion rule.
- Stax style automatically switches the Agent to fastest-lock optimization.
- Added dedicated AI Agent `CHAT` tab.
- Added threaded local-AI deck conversation worker.
- Added grounded chat context using commander data, current deck, style,
  AI Agent report and current AI commander profile.
- Added clickable Send/Clear chat controls.
- Added terminal-native chat input and recent-conversation display.
- Chat and style state reset when a different deck is analyzed.


## 0.19.2

- Replaced the Agent's preset deck-style interview with one free-form multiline build-brief textbox.
- Users can describe playstyle, win conditions, pet cards, themes, speed goals, pod expectations, exclusions, and other deckbuilding preferences in their own words.
- Enter inserts a new line; Ctrl+Enter or F2 starts research and optimization.
- Pasting into the build brief is supported.
- The complete brief is supplied to the local AI before Agent research/simulation and remains visible in the Agent and Chat context.


## 0.19.3

- Added automatic Ollama lifecycle management.
- Commander Lab checks `data/ai_config.json` at startup and only manages
  Ollama when local AI is enabled and points at localhost/127.0.0.1:11434.
- Startup first probes `/api/tags`; an already-running Ollama server is reused
  and deliberately left running when Commander Lab exits.
- If no local server is responding, Commander Lab searches for `ollama.exe`
  through PATH and common Windows install locations.
- Installed Ollama is launched automatically with `ollama serve`.
- Startup waits up to 12 seconds for the local server before falling back to
  the deterministic AI Optimization.
- Ollama server output is hidden so the TUI remains a single-window workflow.
- Commander Lab's status bar reports whether Ollama was already running,
  launched automatically, unavailable, disabled, or configured externally.
- On normal shutdown Commander Lab terminates only the Ollama server it
  launched itself.
- Windows shutdown uses `taskkill /T` on the managed PID so Ollama model-runner
  child processes are not left behind.
- `Drop` remains a shutdown safety net for early-return/unwind paths.


## 0.19.4

- Simplified the AI Optimization page around the free-form build brief.
- Removed Fastest Win/Stax toggle buttons, manual round controls, and Agent
  bracket buttons from the Agent page.
- Agent page now exposes only Quick / Standard / Thorough / Exhaustive plus
  the button to write/edit the build brief.
- Thoroughness automatically determines optimization passes:
  Quick=1, Standard=3, Thorough=5, Exhaustive=8.
- Requested bracket can be written naturally in the brief (for example
  `Bracket 4` or `B5`). If no bracket is requested, Agent optimization uses
  the currently detected deck bracket.
- Stax/lock intent is inferred from the brief; phrases such as `no stax` or
  `avoid stax` prevent accidental Stax-lock optimization.
- AI Commander Profile now extracts `testing_requests` and
  `feedback_preferences` from the user's build brief.
- Agent trace records those test/feedback instructions so the user can tell
  the Agent how results should be evaluated and explained.
- Added automatic Windows Ollama installation when the runtime is missing,
  using Ollama's official `https://ollama.com/install.ps1` installer.
- Added automatic configured-model detection and model download through
  Ollama's local `/api/pull` endpoint.
- Model downloads show percentage/status in the startup console before the
  TUI opens.
- Fixed the installation startup race where the official Ollama installer may
  start its own localhost server before Commander Lab launches `ollama serve`.
- Installer-started Ollama is treated as Commander-Lab-owned because the
  startup probe established that no server existed before installation, and
  it is stopped when Commander Lab exits.


## 0.19.5

- Added persistent user-supplied reference sources.
- Sources screens now expose `ADD USER REFERENCE` and removal controls.
- Moxfield user-reference URLs are imported as full parsed decklists.
- Archidekt user-reference URLs are imported as full parsed decklists.
- Other public HTTP/HTTPS pages are cached as strategic excerpts.
- User references are stored in each deck's `.sources.json` sidecar with
  `user_provided`, priority, optional parsed deck, and cached excerpt fields.
- Existing `.sources.json` files remain compatible through serde defaults.
- Added global `data/source_cache/user_references/` cache keyed by normalized
  URL so the same user reference is not repeatedly downloaded.
- Agent build briefs automatically detect URLs and attach those references
  before research starts.
- Agent Chat automatically detects URLs in messages and exposes newly attached
  references to the same response.
- Local AI receives complete user reference decklists/page excerpts in a
  dedicated high-priority evidence section.
- User references remain in Agent output provenance even when local AI falls
  back to deterministic optimization.
- Fixed the `Self::agent_rounds_for_intensity(...)` associated function call.
- Fixed HTML cleanup in the generic web-reference importer.


## 0.19.6

- Fixed Windows Ollama startup race that could report
  `ollama serve exited early with exit code 1` even though the Ollama desktop
  app/background service successfully claimed port 11434.
- If the managed `ollama serve` child exits early, Commander Lab now probes
  the configured localhost API for another five seconds before declaring
  startup failure.
- A responding background Ollama server is accepted immediately and model
  verification/download continues normally.
- Added startup logging for recovered server races.
- Added a final API probe at the normal startup timeout boundary.
- Improved unrecovered startup error text so genuine Ollama failures are
  distinguishable from port-bind races.


## 0.19.7

- Reworked AI Optimization progress reporting into structured stages.
- Added a bottom ASCII progress bar with real phase/round percentages.
- Agent progress now tracks a concise current task instead of dumping every
  internal worker/debug line into the visible activity feed.
- Completed stages are recorded as `DONE` entries with a summary of what
  actually finished.
- Added explicit progress stages for commander research, user-reference
  attachment, local-AI commander modeling, baseline simulation, candidate
  planning, every candidate build/test pass, and final validation.
- Candidate generator percentages are mapped into the current optimization
  pass's share of the overall Agent progress bar.
- Local-AI initialization/research/synthesis messages advance the overall bar
  through the AI-modeling phase instead of appearing as noisy bracketed tags.
- Internal worker prefixes such as `[AI]`, `[RESEARCH]`, and `[FINAL]` are
  stripped from the user-facing current-task detail.
- Full diagnostic/decision traces remain available in TRACE/logging; the Agent
  screen now focuses on readable task state and completed work.


## 0.19.8

- Added scrolling to long Sources, AI Agent Trace, Agent, Agent Chat, and
  per-deck Log panes.
- Mouse wheel works on the active long-text pane; PageUp/PageDown work on
  Agent/Trace/Log and Up/Down/PageUp/PageDown navigate chat history.
- Sources now honor a persistent source-scroll offset rather than truncating
  at the first visible sources.
- Hovering a visible source changes the selected/highlighted source without
  opening it.
- Deck Log and AI Agent Trace lines register mouse regions and highlight the
  exact visible line under the mouse pointer.
- Agent build-brief input now shows a visible `▌` typing caret plus line,
  column, and character-count indicators.
- Agent build-brief text auto-scrolls to keep the current typing position in
  view as the prompt grows.
- Agent Chat input now shows its own caret, line/column/character position and
  auto-scrolls when the input is taller than the message box.
- Agent conversation history can be scrolled instead of always discarding
  older visible messages.
- Added persistent `<deck-name>.history.json` files beside each deck.
- Per-deck history caches draw simulations, goldfish tests, Commander
  Spellbook checks, AI Agent research, AI Agent simulations, and AI Optimization
  Agent runs.
- Added a Dashboard `LOG` tab that loads that deck's cached test/Agent history
  whenever the deck is opened.
- Cached run records include timestamp, test type, concise result summary, and
  relevant trace/log lines.
- Deck history is capped to the most recent 80 run records to prevent
  unbounded files.


## 0.19.8-b.1

- Moved per-deck history logs into a dedicated `logs/` subfolder under the
  decklist directory.
- Deck history now saves as `decks/logs/<deck-name>.history.json`.
- Existing sibling `<deck-name>.history.json` files are migrated automatically
  into the new `logs/` folder the next time that deck is opened.
- History writes create the `logs/` directory automatically when needed.
- Deleting a deck also removes both its new history file and any legacy
  sibling history file.


## 0.19.9

- Simplified pasted-deck import so it opens directly into the decklist paste
  window instead of asking for a deck name first.
- Replaced the pasted-deck text view with a sliced visible-line renderer;
  Commander Lab no longer constructs/renders the entire pasted buffer every
  frame.
- Bracketed clipboard paste appends the full clipboard payload in one
  operation instead of visually animating individual characters.
- Pasted decklists can be reviewed with mouse wheel, Up/Down, and
  PageUp/PageDown scrolling before import continues.
- F2 or Ctrl+Enter parses the pasted list.
- If the parser cannot identify a commander, Commander Lab explicitly asks
  the user for the commander rather than silently saving an unknown-command
  deck.
- Commander entry uses the same fuzzy/canonical commander resolver as deck
  generation and supports partner-style commander input.
- Resolved commander cards are moved out of the main deck into the command
  zone before save.
- Every pasted deck gets a final naming prompt before it is written.
- Leaving the name blank automatically names the deck after its commander(s).
- If neither a name nor commander exists (only as a defensive fallback), the
  filename becomes `Imported Deck`.


## 0.20.0

- Fixed card-image graphics artifacting during AI Optimization, AI Agent,
  generation, and Agent Chat background work.
- Added an automatic safe-image mode for redraw-heavy background jobs.
- While a worker is active, existing card previews are rebuilt with Ratatui
  Image's Halfblocks protocol so the image is represented by ordinary terminal
  cells rather than a persistent Kitty/Sixel/iTerm graphics plane.
- Halfblocks remain fully visible/click-compatible while tests run, but are
  safely overwritten by Ratatui's normal frame repaint.
- The negotiated high-quality terminal image protocol is restored
  automatically as soon as all background workers finish.
- Card preview rectangles are now explicitly cleared and repainted before each
  image frame to prevent stale image-cell fragments around resized/reflowed
  layouts.
- Safe-image transitions rebuild the currently selected card from the existing
  image cache; card images are not re-downloaded.


## 0.20.1

- Agent Chat can now directly launch the AI Optimization.
- Added Quick / Standard / Thorough / Exhaustive controls to the top of Chat.
- Added `RUN AGENT WITH CHAT DIRECTION` to Chat.
- Recent user chat direction is appended to the base Agent brief before each
  optimization run.
- Text currently typed into the chat box is also committed as direction when
  the Run Agent button is clicked, so users do not have to send it first.
- URLs in that pending direction are attached as user references before the
  optimization run.
- Agent runs launched from Chat remain on the Chat page while running and
  after completion rather than forcing navigation to the separate Agent page.
- Chat shows the same live Agent task/progress bar while optimization runs.
- Successful Agent runs append a concise optimization-result summary to the
  conversation so the user can immediately provide another correction and
  rerun.
- Failed Agent runs also report back into Chat for iterative troubleshooting.
- Added a direct Dashboard `AGENT CHAT` button.
- Removed the standalone Dashboard Goldfish tab and Goldfish action.
- Removed the standalone interactive manual goldfish/playtest window and its
  controls.
- Removed the dashboard `G goldfish` shortcut and Goldfish button.
- The underlying simulation/goldfish engine remains available internally to
  AI Agent and AI Optimization testing.


## 0.20.2

- Renamed the user-facing `Deep Learn` workspace to `AI Agent`.
- Renamed the user-facing `Min/Max Agent` optimization workflow to
  `AI Optimization`.
- Updated buttons, tabs, status messages, progress labels, help text, and
  documentation to use the new terminology.
- Internal Rust identifiers remain unchanged to keep this release focused on
  UI terminology and avoid unnecessary refactoring risk.


## 0.20.3

- Added the Generate Deck delayed fuzzy commander search to the pasted-deck
  commander confirmation screen.
- Pasted-deck commander matches use the same canonical local card database,
  debounce timing, fuzzy ranking, mouse selection, and partner/background
  separator behavior as deck generation.
- Removed the AI Optimization "give brief" modal/button workflow.
- Added a persistent inline `GOAL` textbox directly to the AI Optimization
  screen.
- Clicking the goal textbox enters edit mode; Enter launches AI Optimization
  with the current text.
- The goal textbox grows upward as wrapped lines are added, shrinking the
  results/activity area while editing, then collapses after the run begins.
- Added a terminal-cell scrollbar to the goal textbox once its content exceeds
  the visible height.
- Goal text supports mouse-wheel and keyboard scrolling and keeps the active
  typing position at the newest content.
- Removed unnecessary explanatory copy from pasted-deck, commander-search, and
  AI Optimization screens.
- Removed implementation-detail paste text from the UI.
- Fixed AI Optimization card hover hitboxes. Card regions are now registered
  against the exact rendered card-name span instead of dividing a row into
  guessed equal-width regions.
- AI Optimization hover regions now account for the results pane's current
  scroll offset and actual terminal rectangle, fixing pointer drift on very
  wide/ultrawide terminals.
- Hovered Agent card names now visibly use the selected/highlight style while
  updating the shared card preview.


## 0.20.4

- Added a visible `CONTINUE` button to the pasted decklist window.
- Added a visible `USE COMMANDER` button to pasted-deck commander selection.
- Added the same `USE COMMANDER` button to Generate Deck commander entry.
- Commander autocomplete rows now track mouse hover and visibly highlight the
  commander currently under the pointer.
- Clicking a fuzzy commander match now accepts that commander immediately and
  advances to the next import/generation step instead of only filling the text
  field.
- Partner/background text already present before the last separator is
  preserved when clicking a fuzzy match for the second commander.


## 0.20.5

- Fixed all UI access to the private AI Optimization receiver.
- `ui.rs` now uses the public `is_ai_optimization_running()` accessor instead
  of reading `deep_agent_rx` directly.
- Removed the unnecessary mutable Halfblocks picker binding.
- AI Optimization now keeps the submitted user goal separately from the
  visible editor.
- After Enter starts a run, the goal editor is cleared immediately.
- The same box becomes the live AI activity panel while optimization is
  running.
- Completed tasks append one `DONE` line each.
- The current operation appears as a final `WORKING` line and updates in place
  as the Agent advances.
- The activity box automatically follows the newest task but can also be
  mouse-wheel scrolled independently.
- Before a run, the goal box explains what the user should enter and includes
  a concise example.
- The goal/activity box dynamically grows with its content up to its maximum
  height and keeps its terminal scrollbar.
- Clicking the activity box while optimization is running no longer switches
  it back into edit mode.
- Removed duplicate current-task/status text from beneath the box; the footer
  now only carries the ASCII overall progress bar and Save Best Deck when
  appropriate.


## 0.20.6

- Added a persistent bottom-right runtime HUD on every screen.
- Shows the exact Commander Lab version, process RAM, process CPU, and app
  uptime.
- Wide terminals also show deck count, current deck card count, source count,
  and active worker state.
- Worker states: IDLE, AI OPT, AI TEST, BUILD, and AI CHAT.
- Runtime process sampling is throttled to once per second.
- HUD automatically uses a shorter format on narrower terminals.
- Added `sysinfo` for cross-platform CPU and memory sampling.


## 0.20.7

- AI Optimization now publishes its completed work into the shared AI Agent
  workspace instead of keeping results isolated on the Agent tab.
- Research tab receives the Agent's research corpus, AI commander summary,
  recommended archetype, game plan, community/evidence findings, reference
  consensus, and AI win packages.
- Recommendations now place actual accepted AI Optimization additions/cuts at
  the top, followed by remaining learned suggestions.
- AI priority cards and AI review/avoid cards are also folded into the shared
  recommendation lists.
- Sources discovered by AI Optimization and the local AI model are merged
  into the deck's Sources tab while preserving user references, and the merged
  source corpus is persisted for the deck.
- Simulation and Speed tabs receive the final accepted deck's testing result.
- Trace continues to include the full AI Optimization decision/activity trace.
- Upgrade shows the accepted AI Optimization result and offers `SAVE AI
  OPTIMIZED DECK` when a result exists.
- Added an `COMPARE` tab to the AI Agent workspace.
- Compare shows the current/original deck beside the accepted optimized deck,
  with removals rendered as red `-` entries and additions as green `+`
  entries.
- Compare includes main/sideboard zone labels, quantities, deck counts, and
  unchanged-main-card count.
- Compare supports PageUp/PageDown and mouse-wheel scrolling.
- Card names in Compare are hoverable/clickable through the shared card
  preview system.


## 0.20.8

- AI Agent Chat is now the primary steering interface for AI Optimization.
- Pressing Enter automatically interprets the message as either a question,
  deck direction, research request, or research-then-optimize request.
- Deck-changing directions run the real AI Optimization pipeline automatically
  instead of returning suggestion-only chat text.
- The resulting optimized deck becomes the active AI Agent result and
  continues populating Research, Recommendations, Sources, Simulate, Speed,
  Trace, Upgrade, and Compare.
- Further chat-directed optimization starts from the latest accepted optimized
  deck instead of going back to the original list.
- Shift+Enter inserts a newline in Agent Chat without sending the message.
- Removed the separate `RUN AGENT WITH CHAT DIRECTION` button/action.
- Questions and simple conversational replies remain normal AI chat; actionable
  deckbuilding directions default to AI Optimization.
- Messages such as `research more`, `find more sources`, `dig deeper`, or
  `raise confidence` force a fresh reference-corpus pass while preserving the
  existing per-source cache and archive.
- Forced research advances the reference crawl instead of discarding cached
  source decks, allowing evidence to accumulate without needlessly refetching
  previously collected decks.
- `research more then optimize` automatically chains the fresh research pass
  into AI Optimization.
- Research completion reports previous/new confidence. Confidence only rises
  when additional independent evidence supports the increase.
- AI Agent Chat, the last submitted build brief, the latest research report,
  final simulations, AI commander model, optimized deck, rounds, trace, and
  comparison inputs are now serialized into the deck's existing
  `logs/<deck>.history.json`.
- Reopening a deck restores its previous AI Agent conversation and reusable
  research/optimization state automatically.
- Restored Agent state repopulates all AI Agent tabs and Compare without
  rerunning research or simulations.
- Closing/leaving a deck screen no longer loses Agent conversation history.
- New research/simulation/optimization only runs again when the user explicitly
  asks for new work or provides a new deck-changing direction.


## 0.20.8-opti.2

- AI Agent testing instructions are now authoritative: an explicit run count
  such as `1,000`, `100k`, or `1m` is passed to the simulation instead of
  being clamped to an intensity preset.
- Thorough is the default AI Agent testing profile when the user does not
  specify a workload or another named profile.
- Removed Quick/Standard/Thorough/Exhaustive selector buttons from the AI
  Agent workflow; the normal Simulate button runs Thorough by default.
- Test/simulation intent takes precedence over optimization. A request to test
  the deck does not run AI Optimization unless the user explicitly asks to
  optimize after testing.
- Added four-player cached-meta matchup testing. Requests such as `run 1000
  test games against meta decks and show win rate` run exactly 1,000 pod race
  simulations and report outright win rate, equity win rate, shared-first
  rate, loss rate, average winning turn, and per-opponent head-to-head rates.
- Meta testing reuses cached reference decks and can extend a shallow local
  corpus using the existing reference collector without invoking the deck
  optimizer.
- Meta matchup results populate AI Agent Chat, Simulate, Trace, deck history,
  and are restored with the deck's cached AI session.
- Scryfall Art Series/non-playable memorabilia objects are excluded from card
  import before they can overwrite a playable Oracle-card record.
- Existing stale rows whose type line is exactly `Card` are purged when the
  card database opens, preventing Art Series records from being treated as
  illegal Commander cards.
- Renamed the Theme screen to Settings. It still contains named themes, color
  controls, and edge style.
- Added a price-currency setting. USD is the default everywhere. EUR or MTGO
  TIX are only displayed/used after the user explicitly selects them in
  Settings.
- Card price previews, short prices, dashboard price summaries, deck totals,
  and sideboard totals now respect the selected Settings currency.
- Cardhoarder/MTGO UI is hidden unless MTGO TIX is explicitly selected.


## 0.20.8-opti.3

- Restored `DeepLearnCard` and `DeepLearnReport`, which were accidentally
  removed from `deep_learn.rs` during the opti.2 simulation refactor.
- The restored definitions match the known-good 0.20.8 main-branch data model.
- Retained `Serialize`/`Deserialize` so cached AI Agent research, tests, chat,
  and optimization sessions remain compatible with deck history persistence.
- This fixes the cascading unresolved-import/type errors in `ai.rs`, `app.rs`,
  `deck_history.rs`, `deep_agent.rs`, and `deep_learn.rs`.


## 0.20.8-opti.4

- Switched the default local AI backend from Ollama to llama.cpp.
- Added an AI provider abstraction so the same Agent/profile/chat code can
  speak to llama.cpp's OpenAI-compatible `/v1/chat/completions` endpoint or
  Ollama's `/api/chat` endpoint.
- Existing pre-provider AI configuration is migrated to llama.cpp rather than
  silently starting the previously configured Ollama installation.
- Added a managed llama.cpp runtime. On Windows Commander Lab can install
  llama.cpp with `winget install llama.cpp`, launch the local server, and let
  llama.cpp download the configured GGUF model directly from Hugging Face.
- Default local model is `Qwen/Qwen2.5-7B-Instruct-GGUF:Q4_K_M`.
- Supports both current `llama serve` and legacy/prebuilt `llama-server`
  command layouts.
- Ollama is now opt-in. Its bootstrap has a hard provider guard, so Commander
  Lab does not install, download, start, or pull Ollama models unless OLLAMA
  was explicitly selected in Settings.
- Added a provider selector under Settings > AI Model. Provider changes are
  persisted and apply on the next launch so managed AI processes shut down in
  a deterministic order.
- Rebuilt Settings as a left-side tabbed screen with Theme, AI Model, and
  Pricing tabs.
- Theme now contains named themes, editable color slots, saved themes, and
  square/rounded edge controls together in one pane.
- AI Model contains local provider/model/endpoint configuration and backend
  behavior.
- Pricing contains the USD/EUR/MTGO TIX preference and pricing policy.
- Removed appearance and price controls from the bottom-right Settings footer.
- Added GPU utilization to the persistent runtime HUD.
- NVIDIA systems also report used/total VRAM through `nvidia-smi`; AMD GPU
  utilization is sampled through `rocm-smi` when available.
- GPU monitoring is throttled to once every two seconds to avoid excessive
  process-monitoring overhead.


## 0.20.8-opti.5

- AI Model settings now shows explicit model choices instead of only the
  llama.cpp/Ollama provider toggle.
- Both providers expose Fast (3B), Balanced (7B), Quality (14B), and Max (32B)
  presets. The current model is visibly marked in the settings pane.
- llama.cpp presets use the official Qwen2.5 GGUF repositories with Q4_K_M
  quantization; Ollama presets use the matching qwen2.5 model sizes.
- Selecting a model persists it to `data/ai_config.json`; the new model is
  loaded on the next launch.
- Local AI startup is now a hard prerequisite for TUI startup. Commander Lab
  does not construct or enter Ratatui until the selected backend is ready.
- `LocalAiRuntime::ensure_running` now returns an error when the selected AI
  backend/server/model did not become ready.
- llama.cpp startup checks `/health` and `/v1/models` and blocks until the
  selected server responds.
- First-run llama.cpp/model setup prints pre-TUI startup status while it waits,
  including elapsed wait time during a model download/load.
- Fresh WinGet llama.cpp installations are located without requiring a new
  terminal by checking the WinGet Links and WindowsApps locations in addition
  to PATH.
- llama.cpp startup supports the current unified `llama serve` command and
  legacy/prebuilt `llama-server`.
- Ollama readiness is also validated when Ollama is explicitly selected.


## 0.20.8-opti.6

- llama.cpp startup now inherits the console's stdout/stderr while Commander
  Lab is still in the pre-TUI bootstrap stage.
- Model download, Hugging Face resolution, GGUF loading, GPU offload, context
  allocation, and llama.cpp errors are therefore visible instead of being
  discarded behind a generic elapsed-time message.
- Removed the five-second carriage-return spinner that could obscure
  llama.cpp's own progress output.
- Commander Lab now emits only a low-frequency 30-second readiness reminder
  while llama.cpp is starting, then enters the TUI only after `/health` or
  `/v1/models` responds successfully.


## 0.20.8-opti.7

- Oracle copy-count overrides are now treated as first-class Commander rules. Cards whose Oracle text permits any number (or a specific higher limit such as seven/nine) are exempted from singleton warnings.
- Legal multi-copy engine cards are no longer penalized as cut candidates by analysis/Deep Learn.
- AI Upgrade preserves existing multi-copy theme engines and their quantities instead of replacing the entire stack as though it were one flexible singleton slot.

- AI Optimization candidate breadth is now instruction-driven. Thorough still
  defaults to five candidates, but explicit requests such as `try 12 deck
  candidates`, `generate 20 iterations`, or `compare 30 builds` override the
  default instead of being clamped back to an intensity preset.
- Candidate count is independent from simulation count, so a brief can request
  e.g. `20 candidate decks, then 100,000 final test games`.
- Candidate iteration numbering/counting was widened to `u16`; explicit
  candidate counts are not silently capped at five/eight.
- The Agent now retains the ten highest-scoring valid candidate deck
  iterations from each optimization run.
- Ranked iterations store the full generated deck, quick comparison test
  result, bracket, objective score, and stax result when applicable.
- Top iterations are part of `DeepAgentResult`, so they are persisted in the
  existing per-deck AI history and restored without rerunning the Agent.
- Rebuilt the AI Agent Recommendations tab to contain only ranked deck
  iterations. Removed learned ADD/CUT suggestion lists and all other
  recommendation-tab clutter.
- Recommendations shows ranked objective scores and speed/lock results, the
  complete selected iteration decklist, and the standard card image/data
  preview.
- Keyboard and mouse selection in Recommendations now moves between deck
  iterations rather than the removed ADD/CUT entries.
- Deck Overview now includes a directly hoverable/clickable deck-card list
  beside the card image panel so card art/data can be inspected without
  leaving Overview.
- Dashboard named-card hover now updates card previews, enabling card images
  in Overview and Commander Spellbook detail.
- Combo detail now lists each combo card on its own hoverable row; combo rows
  also retain their existing focus behavior.
- Card Types and Functional Roles were reduced from tall bar charts to compact
  single-line summary panels, freeing substantially more Overview space for
  deck information and card previewing.

## 0.20.8-opti.8

- Reworked Deck Overview layout so Card Types and Functional Roles are stacked in the left column.
- Card list and card image/data preview now consume the full remaining Overview height, substantially increasing vertical card-art space.


## 0.20.8-opti.9

- Hardened local AI JSON parsing so valid model responses no longer break the
  Agent merely because the model wrapped them in Markdown.
- JSON responses are now accepted in four forms: raw JSON, fenced `json`
  blocks, fenced blocks without a language tag, and JSON embedded after/before
  short explanatory text.
- Added a string/escape-aware balanced JSON extractor so braces/brackets inside
  JSON strings do not terminate extraction early.
- AI prompts now explicitly request raw JSON without Markdown fences or
  surrounding prose.
- The deterministic optimizer fallback is now reserved for genuinely
  unparseable/invalid AI output instead of ordinary fenced JSON.


## 0.20.8-opti.10

- Fixed llama.cpp console output corrupting the Ratatui screen after startup.
- llama.cpp stdout/stderr now use pipes instead of inheriting the terminal.
- Background drain threads continuously consume both pipes so llama.cpp cannot
  stall if its output buffer fills.
- During pre-TUI bootstrap, drained llama.cpp lines are still mirrored to the
  console so model download/load diagnostics remain visible.
- The instant `/health` or `/v1/models` reports ready, terminal mirroring is
  disabled before Commander Lab enters the TUI.
- llama.cpp output continues to be consumed silently for the lifetime of the
  server, preventing timing/token log lines from overwriting AI Activity,
  progress bars, controls, or any other TUI region.


## 0.20.8-opti.11

- Reworked Settings > AI Model into compact `SETTING - VALUE` rows instead of
  large provider/model cards.
- AI Provider is now a single toggle row cycling LLAMA.CPP → OLLAMA → API.
- AI Model is now a dropdown/selection-window row. llama.cpp and Ollama open a
  keyboard- and mouse-selectable Fast/Balanced/Quality/Max model picker.
- API mode opens an exact model-name editor from the same AI Model row so
  arbitrary OpenAI-compatible hosted models can be used.
- Added an API provider mode with editable base URL, exact model ID, and API
  key. API requests use Bearer authentication against an OpenAI-compatible
  `/v1/chat/completions` endpoint.
- API base URL/model settings are retained separately, so switching between
  llama.cpp/Ollama/API does not discard a custom hosted-provider setup.
- API keys are masked in the Settings UI and API-key entry modal and are never
  emitted into Agent/AI logs. Submitting an empty key clears the saved key.
- API mode does not start or install llama.cpp/Ollama; it uses the configured
  remote endpoint directly.
- Added a Web Research ON/OFF row to AI settings.
- Added Theme > Background OPAQUE/TRANSPARENT. Transparent mode uses terminal
  default background (`Color::Reset`) for the base, panels, alternate panels,
  modals, and other theme-backed surfaces, allowing terminal transparency or
  wallpaper effects to show through.
- Background transparency is persisted in `data/ui_style.json`.

---

## Detailed release notes (0.4 – 0.20)

Longer write-ups of each release, moved here from the README.

### What 0.4 adds

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

### 0.5.0 additions

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


### 0.5.2 Theme navigation

Theme customization now lives at the application level rather than inside an
individual deck.

Open **THEME** from the main deck-library screen. It opens a dedicated theme
screen. Press **Esc** or click **BACK** to return to the library.


### 0.6.0

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


### 0.6.1 — bracket-targeted generation + deck deletion

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


### 0.7.0 — reference data, Archidekt, RGB pickers, clipboard and price

- Overview shows Scryfall USD total for the main deck and sideboard.
- NEW DECK imports public Archidekt URLs.
- Generator uses a small 24-hour, rate-limited cache of public reference decks
  from Archidekt and best-effort Moxfield search.
- Bracket 5 additionally tries cEDH Decklist Database-linked Moxfield lists.
- Reference frequency is an extra signal, not a replacement for Commander
  rules, archetype fit, structural role targets, bracket limits or Spellbook.
- Theme swatches open an RGB slider picker instead of a hex-only field.
- COPY exports Commander / Deck / Sideboard sections to the system clipboard.


### 0.8.0 — guided coherent generation

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


### 0.8.1 — expanded EDHREC theme selection

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


### 0.8.2 — actual EDHREC top-10 tags

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


### 0.8.3 — themes tailor play style without excluding cards

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


### 0.9.0 — commander-normal-build intelligence

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


### 0.10.0 — keyboard focus, Sources, and AI Agent

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


### 0.11.0 — Taskii-inspired terminal UI overhaul

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


### 0.12.0 — shared AI Agent intelligence

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


### 0.13.0 — explainable AI, deep simulations, combo explorer, named themes

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


### 0.14.0 — Neon Slate / application-shell redesign

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


### 0.15.0 — AI Agent workspace

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


### 0.16.0 — large reference corpus + hard bracket lock

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


### 0.17.0 — AI Optimization AI Optimization + universal AI Agent card preview

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


### 0.17.1 — commander name resolver

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


### 0.17.2 — structured AI Agent and generation logging

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



### 0.18.0 — cumulative source cache + debounced commander autocomplete

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


### 0.18.1 — clickable commander autocomplete

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


### 0.18.2 — runtime commander-rule validation

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


### 0.18.3 — live generation activity, no deck-folder log

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


### 0.19.0 — local MTG AI research/optimization layer

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


### 0.19.1 — Agent strategy interview + deck conversation

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


### 0.19.3 — Ollama starts and stops with Commander Lab

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


### 0.19.4 — brief-driven Agent + complete local-AI bootstrap

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


### 0.19.5 — user-supplied reference decks and pages

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


### 0.19.8 — scrollable text panes and per-deck history

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


### 0.19.9 — faster pasted deck import

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


### 0.20.1 — Chat is the steering wheel for the Agent

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
