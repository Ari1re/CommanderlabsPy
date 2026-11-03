// TypeScript shapes of what the core's JSON API returns. They mirror the
// Rust structs by hand, so when a Rust struct gains a field, add it here.
//
// Things to work on:
// - Generate these from the Rust types (e.g. with `ts-rs`) so they can't
//   drift.

export interface Roles {
  ramp: boolean;
  draw: boolean;
  tutor: boolean;
  removal: boolean;
  counter: boolean;
  wipe: boolean;
  recursion: boolean;
  protection: boolean;
  graveyard: boolean;
}

export interface CardView {
  oracle_id: string;
  name: string;
  mana_cost: string;
  cmc: number;
  type_line: string;
  oracle_text: string;
  color_identity: string;
  edhrec_rank: number | null;
  is_basic: boolean;
  commander_legal: boolean;
  commander_eligible: boolean;
  image_uri: string;
  price_usd: number | null;
  price_eur: number | null;
  price_tix: number | null;
  tcgplayer_uri: string;
  cardmarket_uri: string;
  cardhoarder_uri: string;
  game_changer: boolean;
  roles: Roles;
  kind: string;
  set_code: string | null;
  collector_number: string | null;
  double_faced: boolean;
}

export type Zone = 'commander' | 'main' | 'sideboard' | 'considering';

export interface Entry {
  quantity: number;
  name: string;
  zone: Zone;
  card: CardView | null;
  set_code?: string | null;
  collector_number?: string | null;
  foil?: boolean;
  /** The player's tags ("Ramp", "Win Condition"…), Moxfield style. */
  tags?: string[];
}

export interface Printing {
  set_code: string;
  set_name: string;
  collector_number: string;
  released_at: string;
  image: string;
  finishes: string[];
  price_usd: string | null;
  price_usd_foil: string | null;
  price_eur: string | null;
}

export interface Change {
  name: string;
  delta: number;
  zone: Zone;
}

export interface Version {
  id: number;
  timestamp_ms: number;
  message: string;
  cards: number;
  changes: Change[];
}

export interface GuideSection {
  title: string;
  text: string;
  points: string[];
  cards: string[];
}

export interface PlayGuide {
  summary: string;
  strengths: string[];
  weaknesses: string[];
  sections: GuideSection[];
  has_combos: boolean;
}

export interface Suggestion {
  card: CardView;
  reason: string;
}

export interface Combo {
  cards: string[];
  result: string;
  source: string;
}

export interface GamePlan {
  commanders: string[];
  identity: string;
  primary_tags: string[];
  summary: string;
  combos: Combo[];
  notes: string[];
  /** How the deck was built (deep learning generation). */
  report?: GenerationReport | null;
}

export interface TierCounts {
  core: number;
  synergy: number;
  staple: number;
  could_use: number;
  none: number;
}

export interface GenerationPick {
  card: string;
  tier: 'synergy' | 'core' | 'staple' | 'could_use' | null;
  reasons: string[];
}

export interface GenerationReport {
  engine: string;
  sources: { name: string; status: string; detail: string }[];
  themes: { name: string; from: string }[];
  pool_tiers: TierCounts;
  deck_tiers: TierCounts;
  picks: GenerationPick[];
  near_misses: GenerationPick[];
  weaknesses: string[];
}

export interface FormatCheck {
  format: string;
  label: string;
  cards: number;
  points: number;
  max_points: number | null;
  pointed_cards: [string, number][];
  problems: string[];
}

export interface Deck {
  name: string;
  entries: Entry[];
  plan: GamePlan | null;
  plan_markdown: string | null;
  format: 'commander' | 'pdh' | 'canlander';
  format_label: string;
  format_check: FormatCheck;
}

export interface DeckSummary {
  name: string;
  commanders: string[];
  identity: string;
  cards: number;
  image: string;
  modified_ms: number;
  has_plan: boolean;
  price_usd: number;
  format?: string;
}

export interface Bracket {
  bracket: number;
  label: string;
  game_changers: string[];
  fast_mana: number;
  tutors: number;
  extra_turn_cards: number;
  mass_land_denial: number;
  ruthless_combos: number;
  reasons: string[];
  confidence_note: string;
}

export interface Analysis {
  deck_name: string;
  commanders: string[];
  identity: string;
  total_cards: number;
  sideboard_count: number;
  known_cards: number;
  unknown_cards: string[];
  legal: boolean;
  legality_issues: string[];
  types: Record<string, number>;
  roles: Record<string, number>;
  avg_cmc: number;
  curve: number[];
  pip_demand: number[];
  direct_sources: number[];
  fetch_lands: number;
  cut_candidates: { card: string; score: number; tag_fit: number; reason: string }[];
  add_suggestions: { card: string; role: string; edhrec_rank: number | null; tag_fit: number }[];
  tags: { name: string; score: number; matched_cards: number }[];
  bracket: Bracket;
  total_price_usd: number;
  total_price_eur: number;
  total_price_tix: number;
  unpriced_cards: number;
}

export interface ComboLine {
  id: string;
  cards: string[];
  result: string;
  prerequisites: string[];
  steps: string[];
  spellbook_url: string;
  bracket: number | null;
  almost: boolean;
  /** Pieces the deck doesn't run yet (combos it could add). */
  missing?: string[];
}

export interface Simulation {
  runs: number;
  average_opening_lands: number;
  keepable_opening_pct: number;
  three_lands_by_t3_pct: number;
  four_lands_by_t4_pct: number;
  five_lands_by_t5_pct: number;
  early_ramp_by_t2_pct: number;
}

export interface MpcImage {
  identifier: string;
  name: string;
  source_name: string;
  dpi: number;
  extension: string;
  thumbnail_url: string;
  tags?: string[];
  official?: boolean;
  printing?: string;
}

export interface MpcFace {
  query: string;
  candidates: MpcImage[];
  selected: number;
  pending_ids?: string[];
}

export interface MpcCard {
  name: string;
  quantity: number;
  front: MpcFace;
  back: MpcFace | null;
}

export interface MpcOrder {
  deck_name: string;
  cards: MpcCard[];
  cardback_id: string;
  stock: string;
  foil: boolean;
  total_slots: number;
  bracket: number;
  resolved: number;
  faces: number;
  not_found?: string[];
  warning?: string | null;
}

export interface MpcConfig {
  backend_url: string;
  cardback_id: string;
  preferred_sources: string[];
  minimum_dpi: number;
  stock: string;
  foil: boolean;
  include_sideboard: boolean;
  max_candidates: number;
  include_official: boolean;
  prefer_official: boolean;
  autofill_site: string;
  autofill_browser: string;
}

export interface Source {
  source: string;
  title: string;
  url: string;
  detail: string;
  user_provided: boolean;
  is_deck: boolean;
}

export interface Status {
  version: string;
  platform: string;
  data_dir: string;
  cards: number;
  card_data_ready: boolean;
  decks: number;
  ai_running: boolean;
  build?: number | null;
  format_data_ready?: boolean;
}

export interface UpdateInfo {
  current_version: string;
  current_build: number | null;
  latest_build: number | null;
  latest_version: string;
  available: boolean;
  published_at: string;
  notes: string;
  page_url: string;
  download_url: string | null;
  install: 'auto' | 'download' | 'none';
  /** windows-installer, windows-portable, linux-script, appimage, linux-binary, deb, android, source, manual */
  kind: string;
  how: string;
  checked_at: number;
}
