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

Download the latest build from the [Releases](../../releases) page. Every update to the app is built and published automatically, and once it's installed Commander Lab updates itself (see [Updates](#updates)).

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

The first run installs the desktop app to `~/.local/share/commander-lab/app`, adds **Commander Lab** to your app launcher, and opens it. Running a newer `.sh` updates the install. Your decks are never touched by updates.

- Start it later: `~/.local/share/commander-lab/app/commander-lab.sh`
- Install without starting: `sh CommanderLab-linux-x86_64.sh --extract`
- Open it in your web browser instead of its own window: add `--web`

The release also includes the `.AppImage` and a `.deb` if you prefer those.

### NixOS

The same `.sh` opens the **desktop app** on NixOS too:

```sh
sh CommanderLab-linux-x86_64.sh
```

The desktop app is an AppImage, which NixOS runs through `appimage-run`. If it isn't installed, the launcher gets it from Nix for that run (the first start takes a moment). To skip that, add it to your configuration:

```nix
environment.systemPackages = [ pkgs.appimage-run ];
```

If Nix can't provide it, the launcher says so and opens the browser edition instead. `--web` always opens the browser edition.

Example Hyprland keybind:

```ini
bind = $mainMod, M, exec, ~/.local/share/commander-lab/app/commander-lab.sh
```

The browser edition and the desktop app share the same decks. If the window comes up blank, the launcher already sets `WEBKIT_DISABLE_DMABUF_RENDERER=1`, the usual fix for NVIDIA and some Wayland setups. Set it to `0` to turn that off.

### Android

1. Download `CommanderLab-android.apk` on your phone and open it.
2. Allow installs from this source if Android asks, then tap **Install**.

New builds install over the old one without losing your decks. Everything works on Android except running a local AI model (the AI Agent's simulations still run).

### Build from source

On Linux (NixOS included), clone the repository and run the script:

```sh
git clone https://github.com/Ari1re/CommanderlabsPy
cd CommanderlabsPy
./run-linux.sh              # desktop app
./run-linux.sh --web        # browser edition
./run-linux.sh --update     # git pull, rebuild and start
```

It needs [Rust](https://rustup.rs), [Node.js 20+](https://nodejs.org) and a C compiler. The desktop app also needs the WebKitGTK 4.1 libraries; the script says which packages to install if they're missing (Debian/Ubuntu: `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev`). On NixOS it builds and runs inside the included `shell.nix`, which has everything. Decks are shared with any installed copy.

By hand, on any system:

```sh
npm --prefix frontend ci && npm --prefix frontend run build

# Browser edition (works everywhere):
cargo run --release -p commander-lab-web

# Desktop app (needs the Tauri system libraries, e.g. webkit2gtk on Linux):
cargo build --release -p commander-lab --features tauri/custom-protocol
# or installers: cd desktop && ../frontend/node_modules/.bin/tauri build
```

On NixOS, open the included shell first: `nix-shell`.

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

Each format builds from its own sources:

- **Commander**: EDHREC, public Moxfield and Archidekt decklists, and the cEDH Decklist Database at Bracket 5.
- **Pauper Commander**: PDHREC (card inclusion and synergy for the commander) and Archidekt decks in the Pauper EDH format.
- **Canadian Highlander**: tournament decklists from MTGTop8's Canadian Highlander section and popular Moxfield Highlander decks in your colors.

Commander brackets only apply to Commander. For PDH the power level sets how hard the build pushes. For Canadian Highlander it sets how many points the deck may spend (levels 1–2 none, 3 up to 4, 4 up to 7, 5 all 10).

**Budget**: choose a total for the whole deck ($50, $100, $250, $500 or your own amount) and, if you like, the most any one card may cost. Prices are Scryfall's US dollar (TCGplayer) prices. The total includes the commander, which always stays. Cards over the limit never enter the build. If the finished deck still costs too much, its most expensive cards are swapped for cheaper ones that do the same job at a similar mana value, and expensive lands become cheaper lands in your colors or basics. The deck's notes list the swaps and the final price. Budgets work in every format.

**Your requests** (on the Build screen): cards to **always include** (your pet cards), **combos to include** (pick each card, then add the combo), cards to **leave out**, and **instructions** in your own words. Requested cards go in first and are kept through every later step, even over the per-card budget or a bracket's Game Changer limit (the notes say so). They must be legal and in your colors, and anything that can't go in is explained in the deck notes. Left-out cards never enter the build. Instructions are followed by the AI model when **Let the AI model help** is on, and kept in the notes either way.

**Deep card search** (a switch on the Build screen) reads the rules text of every card in Scryfall's database for cards that fit your play styles, including older cards no decklist runs. A -1/-1 counters deck picks up wither and persist creatures, for example. The deck's notes say which cards came from the search and why.

At **Bracket 5 (cEDH)**, the generator builds from cEDH Decklist Database lists first; casual decklists and EDHREC only break ties. The deck's notes say which sources were used.

Every generated deck is checked before it's saved: exactly 100 cards, singleton, inside the commander's colors, Commander-legal, and locked to the bracket you picked.

### Add an existing deck

**Decks → Add a deck**:

- **Paste a list** in any common format. Set codes, collector numbers and `*F*` foil markers are kept.
- **Deck link** from Moxfield or Archidekt.
- **Share code**: scan a friend's QR code with your camera (or a photo of it), or paste their code.

### Edit

**⋯ → Change commander…** gives the deck a new commander (or partner pair), for example to rebuild a precon around another legend. Choose whether the old commander moves into the deck, the sideboard, Considering, or out. Cards outside the new colors stay and are flagged so you can swap them.

The **Cards** tab is a Moxfield-style editor. It opens read-only; tap **Edit** to change the deck (and **Done editing** to finish). Tags don't show on the cards; choose **Group by Tag** to see the deck in tag sections.

- Type in **Add a card** to search every card and press Enter to add it. Like Moxfield, it takes a quantity and tags: `2x Sol Ring #Ramp #Mana`.
- **Group by** type, tag, mana value or color. Tag chips filter the list; **Auto-tag** tags every card by its role.
- Tick cards (in the list or on their images) to tag, untag, move or remove them together.
- **Drag** a card, or every ticked card, onto a group: onto **Sideboard**, **Considering** or **Commander** to move it, or in **Group by Tag** onto another tag to retag it (**Untagged** clears the tag). On a computer; on a phone, tick cards and use the bar.
- **Keys** for the card under the pointer: **+ / −** quantity, **S** sideboard, **C** Considering, **M** deck, **Delete** remove, **X** tick, **/** jump to Add a card, **Esc** clear the ticks.
- On **Suggestions**, each card can go to the deck (**Add**), **+ Sideboard** or **+ Considering**.
- Use **+ / −** to change counts. The **⋯** menu moves cards to the sideboard, makes a card your commander, or removes it.
- Click any card to see it full size. Choose **Change printing** to pick from every printing, with art and prices, and toggle **Foil**.
- Switch between **List** and **Images**, and sort by mana value or name.
- **⋯ → Edit as text** edits the whole list at once.
- Cards you're thinking about go in **Considering** (a maybeboard). It isn't counted in the deck, its price or legality. Use **⋯ → Move to Considering** or **Move to deck**. A "Maybeboard" section in an imported list lands here.

### Search cards (Scryfall syntax)

The **Search** tab works like Moxfield's card search, using Scryfall's search syntax: `t:creature o:flying mv<=2`, `otag:ramp`, `usd<1`, `is:commander`, and so on.

- **Only cards that fit this deck** adds the deck's color identity and format legality to your search for you.
- Sort by popularity, name, mana value, price, release date, rarity, color or power, in either direction.
- Show one result per card, per art or every printing. Adding from **Every printing** adds that exact printing.
- Quick filters cover common searches: creatures, instants, ramp, card draw, removal, board wipes, tutors, counterspells, cheap cards.
- Each result has **Deck**, **Side** and **Consider** buttons and a badge showing how many you already have. Tap the image to see it full size.
- If the search has a mistake, Scryfall's explanation is shown. **Syntax guide** opens Scryfall's reference.
- When a search finds nothing, Commander Lab suggests searches that do, with how many cards each finds. Card text uses current wording, so `o:unblockable` suggests `o:"can't be blocked"` and `otag:unblockable`, `o:etb` suggests `o:enters`, and a tag that doesn't exist (`otag:wrath`, `otag:card-draw`) suggests the real ones (`otag:board-wipe`, `otag:draw`). Tap one to run it.

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

You can also run a **50,000-game opening-hand simulation**, run a **goldfish test** to see how fast the deck wins, and **upgrade the deck to a bracket** as a new copy. Pick the bracket and the direction to take it: up to three of the commander's popular play styles, or your own words ("Infect", "Landfall", "Elves"). Leave the direction empty to keep its current style.

### Combos

The **Combos** tab checks Commander Spellbook for combos already in your deck and combos you're one card away from, with step-by-step instructions. Each combo shows images of its cards. A card you're missing is outlined and marked **Add this**; tap any card to see it full size and add it. The result also refines the bracket estimate.

Every card in a listed combo is legal in the deck's format and colors, cards you already run included (your commander always counts). A Pauper Commander deck only sees combos made of commons.

- **Have or almost have**: combos in the deck and ones you're a card away from.
- **Could add**: combos you don't run yet, your commander's first.
- **Legal combos**: every combo legal for your commander, colors and game type, up to the bracket you pick (B1–B5, starting at the deck's bracket; Pauper Commander and Canadian Highlander list them all). Cards you already run are marked. **Browse legal combos** works before you've checked the deck.

### Learn how to play it

**How to play** is a guide written from your actual cards:

- what the deck does well and what to improve
- mulligan advice backed by simulation
- your early-turn plan
- your card-advantage engines
- how the deck wins (combos, finishers, what to tutor for), including the win lines common in this commander's decklists that the deck runs
- how to use your interaction
- **Potential changes**: common win lines in this commander's decklists that the deck doesn't run, what to add, why each was left out (format, colors, budget, bracket, or no room), and cards you could cut that aren't part of how the deck wins

Generated decks study those win lines while building: recurring combos and finishers (overruns, drains, alternate wins, extra combats, damage doublers, infect) from the reference decks, and the most common finishers that fit the deck are added. The primer re-checks them against the deck as it is now, so lines you add or remove move between the two lists.

For questions in your own words, use the **AI Agent** chat.

### History: go back to any version

Every change is saved automatically, whether it's an added card, a new printing, an applied suggestion or a text edit. The **History** tab lists every version with what changed (`+1 Sol Ring`, `−1 Mind Stone`). **View list** shows a past version, and **Restore** brings it back. The version you replace stays in the history, so nothing is ever lost.

### Share

**Share** shows a QR code and a share code holding just what rebuilds the deck: the commander and every card with its count. Tick **Include printings and sideboard** to add those too; this makes a bigger code. On another device, scan it or paste the code. No account or server is involved.

### Export

**Export** sends the deck to:

- **Moxfield**: copies the list in Moxfield's import format with each card's set and collector number, and opens Moxfield. Choose **Create deck → Import → paste**.
- **Archidekt**: copies the list in Archidekt's format (the commander goes into the Commander category) and opens Archidekt. Make a **New deck**, then **Import cards → paste**.
- **Tabletop Simulator**: a saved deck object with every card's image, the commander face up beside the deck, and double-faced cards that flip. If Tabletop Simulator is installed on the computer, it's saved straight into its **Saved Objects** folder; in TTS open **Objects → Saved Objects** and spawn it. Otherwise download the file and put it in `Documents/My Games/Tabletop Simulator/Saves/Saved Objects` (Windows) or `~/.local/share/Tabletop Simulator/Saves/Saved Objects` (Linux).

Moxfield and Archidekt don't let other apps create decks in your account, so copy-and-paste is the most direct route.

### Print proxies

The **Proxies** tab builds a MakePlayingCards order using MPC Autofill:

- Click **Find proxy art**, then pick the art for each card. You get every image MPC Autofill has (tap **Load more** for the rest) plus every official artwork of the card from Scryfall, marked **Official**. Filter by All / Official / MPC. Double-faced cards get their back face.
- Official scans aren't on Google Drive, so **Export order** downloads the ones you picked into a folder next to the order and the XML points at those files. Use the MPC Autofill desktop tool (v4 or later) on the same computer for those; the mpcfill.com website can't read local files.
- **Order options** sets card stock, foil, the sideboard, minimum DPI, preferred sources, the cardback and the server.
- **Export order** saves the XML for the MPC Autofill tool or mpcfill.com, plus a plain list for mpcfill.com's text box.

### AI Agent

**Ask about this deck** is a chat at the top of the tab. Ask anything about the deck: what a card does or why it's in, how two cards work together, a rules question, how to mulligan, what to do in the first turns, how it wins, its combos, matchups and weaknesses, its curve, mana, price or bracket, or what to cut and add. Tap a suggested question to start.

- With an AI model, every answer gets the whole deck: each card's rules text and role, the stats, bracket, price, game plan and any combos found on the **Combos** tab.
- Without one, Commander Lab answers from its own analysis: name a card to get its text and why it's in the deck, or ask about the curve, mana, price, bracket, combos, mulligans, early turns, how it wins, interaction, or what to cut and add.
- On Android, connect an online AI under **Settings → AI model** for AI answers.

**Optimize the deck**: tell the agent what to do in your own words, for example "win faster, keep the graveyard package, stay in Bracket 4" or "turn it into a stax deck that locks the table". There's no goal to pick: it reads your request (and your last few chat messages) and optimizes for that. The agent researches the commander and any reference decks you've added under **Sources**, builds candidate versions, goldfishes each one, and ranks them. Save any result as a new deck. Your deck isn't changed until you do.

The AI model is optional. Pick it in **Settings → AI model**:
- the built-in llama.cpp model
- Ollama
- any OpenAI-compatible API

Without one, the agent still runs its research and simulations, and the chat answers from Commander Lab's analysis. On Android only the online API is available.

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

- **Panels face** sets the layout: **Across** (the top row faces the players opposite), **Sides** (panels face the screen's long edges: the left and right columns with the phone upright, the top and bottom when it's held long ways; it follows you when you turn the phone) or **All me**. Change it any time from the center button, and anyone can turn just their own panel with **Rotate my panel** in their menu.

- **Tap** the left or right side of a panel for −1 / +1. **Hold** for −10 / +10.
- **Swipe** up or down on a panel (or tap ⋯) for that player's menu. It has **Set life total**, commander damage taken, poison, commander tax, energy, experience, rad, tickets, custom counters, Monarch/Initiative/City's blessing, background art from any card, and concede.
- **Deal commander damage** turns every other panel into a damage counter for that commander. Tap the players you hit, then **Done with combat**.
- Tap a counter on a panel (poison, tax, ⚔ commander damage, custom counters) to change it right there.
- Tap the ♛ Monarch or ⚔ Initiative token to choose who gets it, or drag it onto another player.
- Turn on **Track turns** to see whose turn it is and pass with **Next turn**. Add a **chess clock** of 10–45 minutes per player.
- The center button has undo, dice, coin, first player, day/night, a life log, a floating mana pool, card lookup, and **End game**. Choosing the winner saves the game; your win rates by player and deck appear under the setup.
- **Leave table** (top of the screen on desktop, or in the center menu) goes back to the app; Esc does the same on desktop. The game is kept if you leave or close the app, and **Resume** brings it back.

### Updates

When a new build is published, Commander Lab shows an **Update** button (in the sidebar on desktop, plus a banner and a dot on Settings). Opening **Settings → Updates** checks right away, and **Check for updates** checks again any time. Every kind of copy updates itself from there:

- **Windows** installer: downloads and installs the update, then reopens the app. The portable exe swaps itself for the new one and restarts.
- **Linux** `.sh` install (NixOS included), AppImage, or the browser-edition binary: replaces itself and restarts.
- **Linux `.deb`**: downloads the new package and installs it (your system asks for your password), then restarts.
- **Built from source** with `./run-linux.sh`: compares your checkout with GitHub, then **Update and rebuild** pulls the new code, rebuilds (a few minutes, with progress shown) and restarts.
- **Android**: downloads the new APK inside the app and opens Android's installer. Tap **Install**; Android updates the app in place and keeps your decks. The first time, Android asks you to allow Commander Lab to install apps; turn it on and come back, and the installer opens.

### Settings

Choose a dark, light or system theme, show prices in USD, EUR or MTGO tix, update card data, and set up the AI model.

**Storage** shows how much space Commander Lab uses and what it's for, and cleans what can be downloaded or rebuilt again, without touching your decks:

- **Deck research**: saved EDHREC, PDHREC, decklist and tournament data (the next build per commander re-downloads it)
- **Card images**, **Printings and tags**, **AI web research**, **Logs**, and **Leftover downloads** (the raw card file and installed updates)
- **AI models** (the built-in AI needs one downloaded again afterwards)
- **Compact the card database**: kept, but compacting reclaims space left by card data updates, often tens of MB

**Select all caches** picks everything except AI models. Decks, their history and plans, exported files, settings, the game log and the reference decks you added are never deleted and are listed as kept.

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
