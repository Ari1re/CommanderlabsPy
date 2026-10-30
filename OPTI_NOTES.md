# opti branch

`opti` is a behavior-preserving cleanup branch created from the exact
Commander Lab 0.20.8 `main` baseline.

## Cleanup policy

- Do not remove user-visible functionality.
- Remove only source that is provably disconnected from the compiled module graph.
- Keep comments for invariants, compatibility constraints, non-obvious algorithms,
  and external-service behavior; remove or condense comments that merely narrate code.
- Keep network caching, AI Agent persistence, simulation, generation, deck comparison,
  card previews, source collection, and all input/mouse behavior unchanged.
- Prefer shared helpers and existing state over duplicate implementations.
- `main` remains the rollback point for the 0.20.8 feature baseline.

## Changes in opti.1

- Removed orphaned `src/playtest.rs`; the manual Goldfish/Playtest UI and module
  had already been removed from `main.rs`, so the file was unreachable.
- Removed unused `TestingIntensity::ALL`.
- Condensed stale/version-history comments while preserving comments that explain
  safety, compatibility, caching, terminal rendering, and scoring invariants.
- Normalized the touched import ordering.
- Replaced stale hard-coded HTTP user-agent version strings with `CARGO_PKG_VERSION`.
- No intentional runtime behavior changes.


## opti.2 behavior improvements

- Explicit Agent test instructions override presets.
- Test-only requests never optimize unless explicitly chained to optimization.
- Added cached meta-pod matchup reporting and persistence.
- Filtered non-playable Scryfall Art Series records.
- Renamed Theme to Settings and made USD the default/explicit price policy.


## opti.3 compile repair

`DeepLearnCard` and `DeepLearnReport` were unintentionally deleted while the
simulation functions were replaced in opti.2. Their exact main-branch field
layout has been restored.


## opti.4 AI/runtime/settings

- llama.cpp is the default managed local AI runtime.
- Ollama is strictly opt-in via Settings > AI Model.
- Settings now uses left navigation with dedicated Theme, AI Model, and
  Pricing panes.
- Runtime diagnostics now include GPU utilization and NVIDIA VRAM.


## opti.5 startup/model selection

- AI Model settings has explicit 3B/7B/14B/32B choices.
- The TUI starts only after the selected local AI backend reports ready.
- Fresh WinGet llama.cpp installs are discovered immediately through WinGet
  link paths as well as PATH.


## opti.6 llama startup visibility

llama.cpp owns the console during pre-TUI startup so the user can see whether
the model is downloading, loading, offloading to GPU, or failing. Commander
Lab still blocks TUI startup until the server reports ready.


## opti.7 candidate ranking / overview

- Explicit Agent candidate counts override the five-candidate Thorough
  default.
- Recommendations is now exclusively a persistent ranked-deck-iteration view.
- Overview and combo detail expose hoverable cards to the shared image panel.
- Card Types / Functional Roles use compact summary strips.


### Commander copy-rule overrides

Cards whose Oracle text overrides deck copy limits are handled as rules
exceptions. Multi-copy theme engines are protected from singleton legality
warnings and from AI cut/replacement logic.


## opti.11 settings/API/transparency

- AI settings use compact setting/value rows.
- Local model choice is a modal selection list rather than four permanent
  buttons.
- API mode supports a configurable OpenAI-compatible endpoint, model and
  masked Bearer key.
- Theme settings can use the terminal's transparent/default background.
