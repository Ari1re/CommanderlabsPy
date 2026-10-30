# Commander Lab

Commander Lab is a Magic: The Gathering **Commander** deck workbench that runs in
your terminal. Give it a commander, a bracket and a play style and it builds a
legal, tested 100-card deck. Or bring your own list and it will analyze it,
suggest cuts and adds, find combos, simulate draws, export it to Moxfield and
turn it into a proxy order for MakePlayingCards.

Everything runs locally. Your decks are plain text files on your own machine.

- [Install and run](#install-and-run)
  - [Windows](#windows)
  - [Linux](#linux)
  - [NixOS](#nixos)
  - [Android](#android)
  - [Build from source](#build-from-source)
- [First launch](#first-launch)
- [What you can do](#what-you-can-do)
- [Controls](#controls)
- [Files and folders](#files-and-folders)
- [Troubleshooting](#troubleshooting)

---

## Install and run

Download the latest build from the [Releases](../../releases) page. A new
release is published automatically every time the code changes.

| Platform | Download |
|---|---|
| Windows 10/11 | `CommanderLab-windows-x86_64.zip` |
| Linux (any distro) | `CommanderLab-linux-x86_64.sh` |
| NixOS | `CommanderLab-linux-x86_64.sh` (see [NixOS](#nixos)) |
| Android 7+ | `CommanderLab-android.apk` |

Commander Lab is a terminal app. It looks best in a modern terminal with true
color and mouse support (Windows Terminal, Kitty, WezTerm, Alacritty, Konsole,
GNOME Terminal). Card art previews use your terminal's graphics protocol when it
has one and fall back to colored blocks when it doesn't.

### Windows

1. Download `CommanderLab-windows-x86_64.zip` and unzip it anywhere, for example
   `Documents\CommanderLab`.
2. Double-click **`CommanderLab.bat`**.

Your decks and card data are saved next to the program, in the same folder. To
update, unzip the new version over the old one; your `decks` and `data` folders
are kept.

If Windows SmartScreen warns about an unknown publisher, choose
**More info → Run anyway**.

### Linux

```sh
sh CommanderLab-linux-x86_64.sh
```

The first run installs the app to `~/.local/share/commander-lab/app` and starts
it. Running a newer `.sh` file updates the install. Your decks are kept in
`~/.local/share/commander-lab` and are never touched by updates.

- Install without starting: `sh CommanderLab-linux-x86_64.sh --extract`
- Start it afterwards: `~/.local/share/commander-lab/app/commander-lab.sh`
- Use a different data folder: `COMMANDER_LAB_HOME=/path/to/folder sh CommanderLab-linux-x86_64.sh`

The Linux build is a single static binary with no library requirements, so it
runs on Debian, Ubuntu, Fedora, Arch, Alpine and the rest without installing
anything.

### NixOS

The Linux build is statically linked, so it runs on NixOS as-is. You don't need
`nix-ld`, `steam-run` or `patchelf`.

```sh
sh CommanderLab-linux-x86_64.sh
```

To launch it from your app launcher or Hyprland/KDE keybinds, point the entry at
the installed launcher:

```sh
~/.local/share/commander-lab/app/commander-lab.sh
```

For example, in Hyprland (`kitty` can be any terminal):

```ini
bind = $mainMod, M, exec, kitty -e ~/.local/share/commander-lab/app/commander-lab.sh
```

Clipboard export works on both Wayland and X11.

To build from source on NixOS instead, open a shell with the Rust toolchain and
a C compiler (needed for the bundled SQLite):

```sh
nix-shell -p cargo rustc gcc
cargo build --release
./target/release/commander-lab
```

### Android

1. Download `CommanderLab-android.apk` on your phone and open it.
2. Allow installing from this source if Android asks, then tap **Install**.
3. Open **Commander Lab**.

This is the full app, running in a built-in terminal. Decks, the card cache and
exports are stored privately on the device. New builds install over the old one
without losing your decks.

On Android:

- **Tap** buttons, tabs and cards just like clicking with a mouse.
- The **key bar** along the bottom gives you `ESC`, `TAB`, arrow keys, `↵`,
  `H`/`L` (switch tabs), `[`/`]` (cycle MPC art), `PGUP`/`PGDN`, `⌨` (show the
  keyboard) and `A-`/`A+` (text size). You can also pinch to zoom.
- The phone's **back button** acts as `ESC`.
- Exports are saved inside the app. The clipboard isn't available to the app,
  so use the export files.
- The local AI model is not used on Android. Everything else works.

Landscape mode gives the dashboard much more room.

### Build from source

You need [Rust](https://rustup.rs) (stable) and a C compiler.

```sh
git clone https://github.com/Ari1re/CommanderlabsPy
cd CommanderlabsPy
cargo build --release
./target/release/commander-lab        # Windows: target\release\commander-lab.exe
```

Run it from the folder where you want your `decks` and `data` folders to live.

---

## First launch

1. Press **`U`** (or click **UPDATE DATA**) once. This downloads the card
   database from Scryfall: card text, legality, prices, art links and printings.
   It takes a minute or two. Run it again whenever you want fresh prices or new
   sets.
2. Add a deck with **`N`** (paste, import or generate). See below.
3. Select the deck and press **Enter** to analyze it.

---

## What you can do

### Add a deck

Press **`N`** on the deck library:

- **Paste decklist.** Paste any list and save with `F2` or `Ctrl+Enter`. Commander
  Lab accepts plain lists, Moxfield and Archidekt exports, set codes and
  collector numbers, foil markers, double-faced card names, and sideboards. It
  finds the commander from a `Commander` section, a `*CMDR*` / `[Commander]`
  marker, the file name, or a 1–2 card final block.
- **Moxfield URL / Archidekt URL.** Paste a public deck link to import it. If
  Moxfield blocks the request, use Moxfield's text export with Paste instead.
- **Generate.** Builds a new deck for you. See below.

Decks are saved as `.txt` files in the `decks` folder. You can also drop `.txt`,
`.dek` or `.deck` files there yourself and press **`R`** to rescan.

### Generate a deck

1. Type a commander. Autocomplete suggests matches and fixes small typos.
   Partners, Backgrounds, Friends Forever and Doctor's Companion pairs are
   supported; enter both names.
2. Pick a **bracket from 1 to 5**.
3. Pick a **play style** from the commander's top EDHREC themes (or a generic
   one).

The generator researches the commander's normal EDHREC build, public Moxfield and
Archidekt lists, Commander Spellbook combos and the commander's own rules text.
It then reserves slots for ramp, card draw, interaction, tutors, protection and
recursion before filling the rest with on-theme cards.

Before the deck is handed to you it must pass two checks:

- **The bracket lock.** The finished list is measured (Game Changers, tutors,
  fast mana, extra turns, mass land denial, combos) and adjusted until it
  matches the bracket you asked for. A deck is never saved under the wrong
  bracket.
- **The integrity test.** The deck must be exactly 100 cards, singleton,
  inside the commander's color identity, and fully Commander-legal. Anything that
  breaks those rules is fixed automatically and checked again. Combo pieces and
  the commander are never cut to make room. It also warns you if the land
  count, ramp, card draw or interaction looks thin, and goldfishes 400 opening
  hands to check how often you'd keep.

The deck's notes show the plan, the combos it found, the sources it used, and
the results of both checks.

### Analyze a deck

Select a deck and press **Enter**. The dashboard has these tabs (switch with
`H`/`L` or click them):

| Tab | What it shows |
|---|---|
| **OVERVIEW** | Card types, roles, legality, bracket, detected archetypes, and a hoverable card list with art |
| **CARDS** | Main deck or sideboard browser (`B` switches) with prices and art |
| **MANA** | Colored sources compared to the pips your spells need |
| **CURVE** | Mana curve |
| **ROLES** | Ramp, draw, removal, counters, wipes, tutors, protection, recursion |
| **LEGALITY** | Commander construction checks |
| **CUT / ADD** | Suggested cuts and replacements that fit your archetype. `Enter` selects, `B` moves a card to the sideboard |
| **BRACKET** | Estimated bracket and the cards that drive it |
| **SIM** | 50,000-game draw simulation: keepable hands, land drops, early ramp |
| **COMBOS** | Combos in your deck and combos one card away, from Commander Spellbook. `O` opens the combo page |
| **SOURCES** | Every website used for the deck. `+` adds your own reference deck or page, `Delete` removes it |
| **MPC** | Proxy order builder. See [Print proxies](#print-proxies-with-mpc-autofill) |
| **LOG** | Per-deck history of tests and agent runs |
| **HELP** | Controls |

Click any card name to see its art, Oracle text, EDHREC rank and price. The
TCGplayer, Cardmarket and Cardhoarder buttons open that card in your browser.

### Export to Moxfield

Press **`M`** on any deck tab, or click **MOXFIELD** in the MPC tab. The list is
saved to `exports/<deck>.moxfield.txt` and copied to your clipboard. Each card
includes its set code and collector number, so Moxfield shows the same printing.

In Moxfield, go to **Create Deck → Import** and paste.

### Print proxies with MPC Autofill

The **MPC** tab turns your deck into an order for MakePlayingCards using the
MPC Autofill image library.

1. Open the **MPC** tab and press **`F`** (or click **SEARCH**). Commander Lab
   finds proxy art for every card. Double-faced cards get their back face
   automatically.
2. Browse the list with `↑`/`↓`. The art panel shows the image that will
   actually print. Press **`[`** / **`]`** to try other art, and **`V`** to look
   at back faces.
3. Choose **`T`** foil, **`O`** card stock, and **`B`** to include or leave out the
   sideboard.
4. Press **`X`** (or click **EXPORT XML**). This saves:
   - `exports/<deck>.mpc.xml`, for the MPC Autofill desktop tool or the import
     button on mpcfill.com
   - `exports/<deck>.mpc.txt`, a plain list you can paste into mpcfill.com. It is
     written even if the search couldn't reach MPC Autofill.

The header shows how many cards are in the order and which MPC bracket size it
rounds up to (for example, 100 cards → 108).

Optional settings live in `data/mpc_config.json`:

| Setting | Meaning |
|---|---|
| `backend_url` | MPC Autofill server to search |
| `cardback_id` | Image ID of your preferred cardback (blank = the server's default) |
| `preferred_sources` | Artists/sources to rank first, e.g. `["Chilli_Axe"]` |
| `minimum_dpi` | Skip images below this resolution |
| `stock`, `foil`, `include_sideboard` | Defaults for new orders |
| `max_candidates` | How many art options to keep per card |

### AI Agent (optional, desktop)

If you run a local model, the **AI Agent** can research your deck in depth,
simulate games against reference decks, and propose upgrades. Use **AI AGENT**
on the deck library or dashboard.

- **Agent brief.** Describe what you want ("faster, keep the graveyard
  package, stay in Bracket 4"). Pasting a Moxfield or Archidekt link in the
  brief uses that list as a reference.
- **Chat.** Talk through changes, then click **RUN AGENT WITH CHAT DIRECTION**.
  Choose QUICK, STANDARD, THOROUGH or EXHAUSTIVE testing.
- **Recommendations.** Ranked deck versions with win-speed and consistency
  results. Save the best one as a new deck.
- **Upgrade.** Rebuild the deck at a chosen bracket. Upgrades go through the
  same bracket lock and integrity test as new decks.

Pick the model in **Settings → AI Model**:

- **LLAMA.CPP** — downloads and runs a local model for you (Fast, Balanced,
  Quality or Max)
- **OLLAMA** — uses your Ollama install (`ollama pull qwen2.5:7b-instruct`)
- **API** — any OpenAI-compatible endpoint (base URL, model name and key)

The AI is optional. If no model is available, Commander Lab says so at startup
and everything else keeps working: generation, analysis, combos, simulations,
MPC and exports. To skip the AI on purpose, start with `COMMANDER_LAB_NO_AI=1`.

### Settings

Press **`T`** on the deck library.

- **Theme.** Recolor the whole UI, save named themes, switch rounded or square
  borders, and choose an opaque or transparent background (lets your terminal's
  blur or wallpaper show through).
- **AI Model.** Provider, model, API settings, and web research on or off.
- **Pricing.** Currency for prices (USD, EUR, MTGO tix).

---

## Controls

Mouse clicks work everywhere. The keyboard shortcuts are:

**Deck library**

| Key | Action |
|---|---|
| `↑` `↓` / `J` `K` | Select deck |
| `Enter` | Analyze |
| `N` | New deck (paste / import / generate) |
| `R` | Rescan the `decks` folder |
| `U` | Update card data from Scryfall |
| `D` / `Delete` | Delete deck |
| `T` | Settings |
| `Q` | Quit |

**Deck dashboard**

| Key | Action |
|---|---|
| `H` `L` / `←` `→` | Previous / next tab |
| `↑` `↓` / `J` `K` | Move within the tab |
| `Enter` | Select / open |
| `S` | Run the draw simulation |
| `C` | Look up combos |
| `M` | Export to Moxfield |
| `R` | Re-analyze |
| `Esc` | Back to the deck library |

**MPC tab**

| Key | Action |
|---|---|
| `F` / `Enter` | Search MPC Autofill for art |
| `[` `]` | Previous / next art for the selected card |
| `V` | Show front or back faces |
| `T` | Foil on/off |
| `O` | Cycle card stock |
| `B` | Include/exclude sideboard |
| `X` | Export the order (XML + text) |
| `PgUp` `PgDn` | Scroll the order |

---

## Files and folders

Commander Lab keeps everything in its working folder. That's the program folder
on Windows, `~/.local/share/commander-lab` on Linux and NixOS, and private app
storage on Android.

| Path | Contents |
|---|---|
| `decks/` | Your decklists (`.txt`). Back this folder up. |
| `exports/` | Moxfield lists and MPC orders |
| `data/cards.sqlite` | Card database from Scryfall (rebuilt by UPDATE) |
| `data/images/` | Cached card and proxy art |
| `data/mpc_config.json` | MPC Autofill settings |
| `data/ai_config.json` | AI model settings |
| `data/theme.json`, `data/ui_style.json` | Theme |
| `data/source_cache/` | Cached research (EDHREC, Spellbook, reference decks) |
| `data/logs/` | Logs, useful when reporting a problem |

---

## Troubleshooting

**"No local card data yet."** Press `U` once to download the card database.

**Exports have no set codes.** Your card database predates printing data.
Press `U` once to refresh it.

**MPC search fails.** Check your internet connection and the `backend_url` in
`data/mpc_config.json`. You can still press `X` and paste
`exports/<deck>.mpc.txt` into mpcfill.com.

**Card art shows as blocks or not at all.** Your terminal doesn't support inline
images. Try Kitty, WezTerm or Windows Terminal. Everything else works the same.

**The app looks cramped.** Make the terminal window bigger or zoom out. On
Android, use landscape or `A-`.

**Moxfield import fails.** Moxfield sometimes blocks automated requests. Use
Moxfield's export to copy the deck as plain text, then add it with
**Paste decklist**.

**The AI Agent says the model is offline.** Open **Settings → AI Model** and
pick a provider, or keep using the app without it. Nothing else depends on it.

See [CHANGELOG.md](CHANGELOG.md) for what changed in each version.
