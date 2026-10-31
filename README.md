# Commander Lab

Commander Lab is an all-in-one app for Magic: The Gathering **Commander** decks, including **Pauper Commander (PDH)** and **Canadian Highlander**. You can:

- **generate** a deck from a commander (or colors, for Highlander), a power level and a play style
- **build** one card by card
- **import** a list, a Moxfield or Archidekt link, or a deck a friend shared by QR code

Then edit it, choose printings, see what to add, find combos, learn how to pilot it, and go back to any earlier version.

It runs on Windows, Linux (NixOS included) and Android. Your decks stay on your own device.

- [Install and run](#install-and-run)
  - [Windows](#windows)
  - [Linux](#linux)
  - [NixOS](#nixos)
  - [Android](#android)
  - [Build from source](#build-from-source)
- [First launch](#first-launch)
- [What you can do](#what-you-can-do)
- [Where your data lives](#where-your-data-lives)
- [Troubleshooting](#troubleshooting)

---

## Install and run

Download the latest build from the [Releases](../../releases) page. Every update to the app is built and published automatically.

| Platform | Download |
|---|---|
| Windows 10/11 | `CommanderLab-windows-x86_64-setup.exe` (installer) or `…-portable.exe` |
| Linux, any distro | `CommanderLab-linux-x86_64.sh` |
| NixOS | `CommanderLab-linux-x86_64.sh` (see [NixOS](#nixos)) |
| Android 7+ | `CommanderLab-android.apk` |

### Windows

Run `CommanderLab-windows-x86_64-setup.exe` and open **Commander Lab** from the Start menu. If SmartScreen warns about an unknown publisher, choose **More info → Run anyway**.

If you'd rather not install anything, the portable `.exe` runs as-is.

### Linux

```sh
sh CommanderLab-linux-x86_64.sh
```

The first run installs the app to `~/.local/share/commander-lab/app` and opens it. Running a newer `.sh` updates the install. Your decks are never touched by updates.

- Start it later: `~/.local/share/commander-lab/app/commander-lab.sh`
- Install without starting: `sh CommanderLab-linux-x86_64.sh --extract`
- Open it in your web browser instead of its own window: add `--web`

The release also includes the `.AppImage` and a `.deb` if you prefer those.

### NixOS

The same `.sh` works. On NixOS it opens Commander Lab in your **web browser**, using a single static binary with no dependencies, so you don't need `nix-ld` or an FHS environment.

```sh
sh CommanderLab-linux-x86_64.sh
```

If you want the native desktop window, install `appimage-run` and the launcher will use it automatically:

```sh
nix-shell -p appimage-run --run "sh CommanderLab-linux-x86_64.sh"
# or add appimage-run to environment.systemPackages
```

Example Hyprland keybind:

```ini
bind = $mainMod, M, exec, ~/.local/share/commander-lab/app/commander-lab.sh
```

The browser edition and the desktop window share the same decks.

### Android

1. Download `CommanderLab-android.apk` on your phone and open it.
2. Allow installs from this source if Android asks, then tap **Install**.

New builds install over the old one without losing your decks. Everything works on Android except running a local AI model (the AI Agent's simulations still run).

### Build from source

You need [Rust](https://rustup.rs), [Node.js 20+](https://nodejs.org), and a C compiler.

```sh
git clone https://github.com/Ari1re/CommanderlabsPy
cd CommanderlabsPy
npm --prefix frontend ci && npm --prefix frontend run build

# Browser edition (works everywhere):
cargo run --release -p commander-lab-web

# Desktop window (needs the Tauri system libraries, e.g. webkit2gtk on Linux):
cd desktop && ../frontend/node_modules/.bin/tauri build
```

On NixOS, open a shell first: `nix-shell -p cargo rustc gcc nodejs`.

---

## First launch

1. Click **Download card data**. Commander Lab fetches Scryfall's card database (about a minute). Refresh it any time from **Settings** for new sets and prices. If you used a version before 0.27, update the card data once to use Pauper Commander and Canadian Highlander.
2. Add a deck: **Build a deck** to generate one, or **Add a deck** to paste, link or scan one.

---

## What you can do

### Generate a deck

**Build** → choose a **format**, then pick a commander (partners and backgrounds supported), a **power level from 1 to 5** and up to three **play styles** taken from that commander's most popular EDHREC themes. The first style you pick is the main focus. Type your own play style in the box under the list ("-1/-1 Counters", "Elves", "Landfall") to build around anything. Commander Lab researches how the commander is usually built and assembles a 100-card deck.

Formats:

| | Commander | Pauper Commander (PDH) | Canadian Highlander |
|---|---|---|---|
| Leader | Legendary commander | Creature, Vehicle or Spacecraft printed at uncommon | None: pick colors |
| Other cards | Commander-legal | Printed at common (paper or MTGO printings) | Vintage card pool, at most 10 points |
| Deck | Exactly 100, singleton | Exactly 100, singleton | 100 or more, singleton, no sideboard |
| Life | 40, 21 commander damage | 30, 16 commander damage | 20, one-on-one |
| Banned | Commander banned list | Mystic Remora, Rhystic Study, ante, Conspiracies, cards removed from constructed | Vintage banned list |

Commander brackets only apply to Commander. For PDH the power level sets how hard the build pushes. For Canadian Highlander it sets how many points the deck may spend (levels 1–2 none, 3 up to 4, 4 up to 7, 5 all 10).

**Deep card search** (a switch on the Build screen) reads the rules text of every card in Scryfall's database for cards that fit your play styles, including older cards no decklist runs. A -1/-1 counters deck picks up wither and persist creatures, for example. The deck's notes say which cards came from the search and why.

At **Bracket 5 (cEDH)**, the generator builds from cEDH Decklist Database lists first; casual decklists and EDHREC only break ties. The deck's notes say which sources were used.

Every generated deck is checked before it's saved: exactly 100 cards, singleton, inside the commander's colors, Commander-legal, and locked to the bracket you picked.

### Add an existing deck

**Decks → Add a deck**:

- **Paste a list** in any common format. Set codes, collector numbers and `*F*` foil markers are kept.
- **Deck link** from Moxfield or Archidekt.
- **Share code**: scan a friend's QR code with your camera (or a photo of it), or paste their code.

### Edit

The **Cards** tab groups the deck by type, like Moxfield.

- Type in **Add a card** to search every card and press Enter to add it.
- Use **+ / −** to change counts. The **⋯** menu moves cards to the sideboard, makes a card your commander, or removes it.
- Click any card to see it full size. Choose **Change printing** to pick from every printing, with art and prices, and toggle **Foil**.
- Switch between **List** and **Images**, and sort by mana value or name.
- **⋯ → Edit as text** edits the whole list at once.

### See cards you could add

The **Suggestions** tab shows card images you can add with one tap. They're grouped as:

- **Completes a combo**: you're one card away.
- **High synergy with your commander**: from EDHREC.
- **Popular in decks like yours**.
- **Fills a gap**: ramp, draw or removal your deck is short on.

- **Found by rules text**: cards whose rules text fits the deck's play styles.

Filter by role (ramp, draw, removal, creatures, lands, …) and by maximum price. To dig for anything else, type an ability or play style in **Dig deeper** ("flash", "wither", "Elves"). Suggestions only include cards legal in the deck's format.

The deck's format shows next to its card count. Change it from **⋯ → Format**; the deck is then checked against that format's rules.

### Analyze

The **Analysis** tab covers:

- mana curve
- color sources compared to the colored symbols your spells need
- role counts
- card types and detected themes
- the bracket estimate with its reasons and Game Changers
- legality problems
- suggested cuts and adds

You can also run a **50,000-game opening-hand simulation**, run a **goldfish test** to see how fast the deck wins, and **rebuild the deck at another bracket** as a new copy.

### Combos

The **Combos** tab checks Commander Spellbook for combos already in your deck and combos you're one card away from, with step-by-step instructions. The result also refines the bracket estimate.

### Learn how to play it

**How to play** is a guide written from your actual cards:

- what the deck does well and what to improve
- mulligan advice backed by simulation
- your early-turn plan
- your card-advantage engines
- how the deck wins (combos, finishers, what to tutor for)
- how to use your interaction

For questions in your own words, use the **AI Agent** chat.

### History: go back to any version

Every change is saved automatically, whether it's an added card, a new printing, an applied suggestion or a text edit. The **History** tab lists every version with what changed (`+1 Sol Ring`, `−1 Mind Stone`). **View list** shows a past version, and **Restore** brings it back. The version you replace stays in the history, so nothing is ever lost.

### Share

**Share** shows a QR code and a share code holding just what rebuilds the deck: the commander and every card with its count. Tick **Include printings and sideboard** to add those too; this makes a bigger code. On another device, scan it or paste the code. No account or server is involved.

### Export to Moxfield

**Export to Moxfield** copies the list in Moxfield's import format, with each card's chosen set and collector number. In Moxfield: **Create deck → Import → paste**.

### Print proxies

The **Proxies** tab builds a MakePlayingCards order using MPC Autofill:

- Click **Find proxy art**, then pick the art for each card. You get every image MPC Autofill has (tap **Load more** for the rest) plus every official artwork of the card from Scryfall, marked **Official**. Filter by All / Official / MPC. Double-faced cards get their back face.
- Official scans aren't on Google Drive, so **Export order** downloads the ones you picked into a folder next to the order and the XML points at those files. Use the MPC Autofill desktop tool (v4 or later) on the same computer for those; the mpcfill.com website can't read local files.
- **Order options** sets card stock, foil, the sideboard, minimum DPI, preferred sources, the cardback and the server.
- **Export order** saves the XML for the MPC Autofill tool or mpcfill.com, plus a plain list for mpcfill.com's text box.

### AI Agent

Describe what you want, for example "win faster, keep the graveyard package, stay in Bracket 4". The agent researches the commander and any reference decks you've added under **Sources**, builds candidate versions, goldfishes each one, and ranks them. Save any result as a new deck. Your deck isn't changed until you do.

The AI model is optional. Pick it in **Settings → AI model**:
- the built-in llama.cpp model
- Ollama
- any OpenAI-compatible API

Without one, the agent still runs its research and simulations.

### Game: life and counters

The **Game** tab turns your phone into a life counter for 2–6 players.

- **Game mode** sets starting life, commander damage and the usual player count:
  - Commander: 40 life, 21 commander damage
  - Pauper Commander: 30 life, 16 commander damage
  - PDH one-on-one: 25 life, 21 commander damage
  - Canadian Highlander: 20 life, one-on-one
  - Brawl: 25 life
  - Oathbreaker: 20 life
  - 60-card formats: 20 life

  Modes without commander damage hide it.

- **Panels face** sets the layout: **Across** (the top row faces the players opposite), **Sides** (the left and right columns face the long edges of the table) or **All me**. Change it any time from the center button, and anyone can turn just their own panel with **Rotate my panel** in their menu.

- **Tap** the left or right side of a panel for −1 / +1. **Hold** for −10 / +10.
- **Swipe** up or down on a panel (or tap ⋯) for that player's menu. It has **Set life total**, commander damage taken, poison, commander tax, energy, experience, rad, tickets, custom counters, Monarch/Initiative/City's blessing, background art from any card, and concede.
- **Deal commander damage** turns every other panel into a damage counter for that commander. Tap the players you hit, then **Done with combat**.
- Tap a counter on a panel (poison, tax, ⚔ commander damage, custom counters) to change it right there.
- Tap the ♛ Monarch or ⚔ Initiative token to choose who gets it, or drag it onto another player.
- Turn on **Track turns** to see whose turn it is and pass with **Next turn**. Add a **chess clock** of 10–45 minutes per player.
- The center button has undo, dice, coin, first player, day/night, a life log, a floating mana pool, card lookup, and **End game**. Choosing the winner saves the game; your win rates by player and deck appear under the setup.
- **Leave table** (top of the screen on desktop, or in the center menu) goes back to the app; Esc does the same on desktop. The game is kept if you leave or close the app, and **Resume** brings it back.

### Settings

Choose a dark, light or system theme, show prices in USD, EUR or MTGO tix, update card data, and set up the AI model.

---

## Where your data lives

| | Folder |
|---|---|
| Windows | `%APPDATA%\CommanderLab` |
| Linux / NixOS | `~/.local/share/commander-lab` |
| Android | the app's private storage |

Inside it are:
- `decks/`: your decklists as plain `.txt` files, plus each deck's version history in `decks/.versions`
- `exports/`: Moxfield lists and MPC orders
- `data/`: card database, image and research caches, and settings

Back up `decks/` to keep your decks and their history. Set `COMMANDER_LAB_HOME` to use a different folder.

---

## Troubleshooting

**"Download the card database first."** Click **Download card data** on the home screen, or **Settings → Update card data**.

**Card images don't show.** Images come from Scryfall, so you need an internet connection. Everything else works offline once the card data is downloaded.

**Moxfield import fails.** Moxfield sometimes blocks automated requests. Copy the deck as text from Moxfield's export and paste it instead.

**Proxy search fails.** Check your connection and the server under **Proxies → Order options**. **Export order** still saves the plain list for mpcfill.com.

**The QR scanner can't open the camera.** Allow camera access, or use **Choose a photo of the QR**. You can also paste the share code.

**The deck is "too large for one QR code".** Send the share code as text instead; it works the same way.

See [CHANGELOG.md](CHANGELOG.md) for what changed in each version.
