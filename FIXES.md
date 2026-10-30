# Fixes in this build

- Fixed `rusqlite::Row::get::<u64>` compilation failure.
  - SQLite `COUNT(*)` is now read as `i64` and safely converted to `u64`.
- Removed unused `DeckEntry` import.
- Removed obsolete unused gauge helper.
- Removed unused `ParsedDeck.path` and `CardDb.path`.
- Removed unused `Card.oracle_id` from the runtime model.
- Made `Chart` Y-axis labels explicit `Line` values to avoid type inference issues.
- Fixed Commander Spellbook parsing when the response is wrapped in `results`.
- Kept all visualization screens on native Ratatui widgets.
- Switched crate edition to Rust 2021 for broader compiler compatibility.


## Scryfall bulk-data parsing fix

- `BulkItem.download_uri` is now `Option<String>`.
- Bulk-data records without a `download_uri` no longer abort JSON deserialization.
- The updater specifically selects a downloadable `oracle_cards` record.
- A failed Scryfall update no longer exits Commander Lab; the error is shown in the TUI status bar.


## Direct Scryfall bulk endpoint

The updater no longer parses `GET /bulk-data`.

It now requests the Oracle Cards metadata directly:

`GET https://api.scryfall.com/bulk-data/oracle_cards`

That response supplies the `download_uri` for the current Oracle Cards JSON file.


## 0.10.0 UI/research fixes

- CUT / ADD, SIM and COMBOS now retain visible keyboard selection highlights.
- H/L tab switching remains available while arrow keys operate page-local focus.
- Generation source provenance is persisted instead of being lost after build.
- Deep Learn research is kept separate from generation so the user can inspect
  learned evidence before deciding to change a deck.
