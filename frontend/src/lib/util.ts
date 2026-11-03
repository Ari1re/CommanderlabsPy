// Small shared helpers: card images, prices and money formatting, bracket
// and power-level names, format list, card-type ordering.

import type { CardView, Entry } from './types';

export const MANA = ['W', 'U', 'B', 'R', 'G'] as const;
export const MANA_NAME: Record<string, string> = {
  W: 'White',
  U: 'Blue',
  B: 'Black',
  R: 'Red',
  G: 'Green',
  C: 'Colorless',
};

/** Scryfall art crop for a "normal" image URL. */
export function artCrop(url: string | undefined | null): string {
  if (!url) return '';
  return url.replace('/normal/', '/art_crop/').replace('/large/', '/art_crop/');
}

/** Split "{2}{U}{U/B}" into symbol tokens. */
export function manaSymbols(cost: string): string[] {
  return [...(cost ?? '').matchAll(/\{([^}]+)\}/g)].map((m) => m[1]);
}

/** Scryfall's SVG for a mana symbol ("U/B" becomes UB.svg). */
export function symbolUrl(sym: string): string {
  return `https://svgs.scryfall.io/card-symbols/${sym.replace(/\//g, '').toUpperCase()}.svg`;
}

export type Currency = 'usd' | 'eur' | 'tix';

/** The card's price in the chosen currency (null when Scryfall has none). */
export function price(card: CardView | null | undefined, currency: Currency): number | null {
  if (!card) return null;
  return currency === 'eur' ? card.price_eur : currency === 'tix' ? card.price_tix : card.price_usd;
}

/**
 * Format a price for display: locale currency for USD/EUR, "1.23 tix" for
 * MTGO, an em dash when unknown.
 */
export function money(value: number | null | undefined, currency: Currency): string {
  if (value == null) return '—';
  if (currency === 'tix') return `${value.toFixed(2)} tix`;
  return new Intl.NumberFormat(undefined, {
    style: 'currency',
    currency: currency === 'eur' ? 'EUR' : 'USD',
    maximumFractionDigits: 2,
  }).format(value);
}

export const KIND_ORDER = [
  'Commander',
  'Creature',
  'Planeswalker',
  'Battle',
  'Instant',
  'Sorcery',
  'Artifact',
  'Enchantment',
  'Land',
  'Other',
];

/** "1 card" / "3 cards". Pass `many` for irregular plurals. */
export function plural(n: number, word: string, many = word + 's') {
  return `${n} ${n === 1 ? word : many}`;
}

/**
 * Rough relative time ("5 min ago", "3 days ago"), then a plain date past
 * a month.
 */
export function timeAgo(ms: number): string {
  const diff = Date.now() - ms;
  const min = Math.round(diff / 60000);
  if (min < 1) return 'just now';
  if (min < 60) return `${min} min ago`;
  const h = Math.round(min / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.round(h / 24);
  if (d < 30) return `${d} days ago`;
  return new Date(ms).toLocaleDateString();
}

export const BRACKETS: { n: number; name: string; blurb: string }[] = [
  { n: 1, name: 'Exhibition', blurb: 'Themed and casual. No Game Changers, combos or tutors.' },
  { n: 2, name: 'Core', blurb: 'Precon-level power. Few tutors, no early combos.' },
  { n: 3, name: 'Upgraded', blurb: 'Stronger synergy, up to three Game Changers, late combos.' },
  { n: 4, name: 'Optimized', blurb: 'High power. Fast mana, tutors and combos allowed.' },
  { n: 5, name: 'cEDH', blurb: 'Competitive. The fastest, most consistent lists.' },
];

export type FormatId = 'commander' | 'pdh' | 'canlander';

export const FORMATS: { id: FormatId; label: string; short: string; summary: string }[] = [
  { id: 'commander', label: 'Commander', short: 'EDH', summary: '100 cards led by a legendary commander. 40 life, 21 commander damage.' },
  {
    id: 'pdh',
    label: 'Pauper Commander',
    short: 'PDH',
    summary: 'An uncommon creature leads 99 commons (paper or MTGO printings). 30 life, 16 commander damage. Mystic Remora and Rhystic Study are banned.',
  },
  {
    id: 'canlander',
    label: 'Canadian Highlander',
    short: 'CH',
    summary: '100+ singleton cards, no commander or sideboard, one-on-one at 20 life. Vintage banned list and at most 10 points from the points list.',
  },
];

/** Display name for a format id. Unknown or missing ids mean Commander. */
export function formatLabel(id: string | undefined | null): string {
  return FORMATS.find((f) => f.id === id)?.label ?? 'Commander';
}

/** Power levels outside Commander, where brackets don't apply. */
export const POWER_LEVELS: Record<'pdh' | 'canlander', { n: number; name: string; blurb: string }[]> = {
  pdh: [
    { n: 1, name: 'Casual', blurb: 'Theme first, gentle interaction.' },
    { n: 2, name: 'Focused', blurb: 'A clear plan with a solid core.' },
    { n: 3, name: 'Tuned', blurb: 'Efficient staples around the theme.' },
    { n: 4, name: 'Strong', blurb: 'Lean, fast and interactive.' },
    { n: 5, name: 'Max', blurb: 'The most efficient commons available.' },
  ],
  canlander: [
    { n: 1, name: 'Casual', blurb: 'No pointed cards.' },
    { n: 2, name: 'Budget power', blurb: 'No pointed cards, tighter curve.' },
    { n: 3, name: 'Tuned', blurb: 'Up to 4 points.' },
    { n: 4, name: 'Strong', blurb: 'Up to 7 points.' },
    { n: 5, name: 'Max', blurb: 'All 10 points.' },
  ],
};

/** Image for a specific printing (Scryfall redirects to the image). */
export function printingImage(set: string, number: string): string {
  return `https://api.scryfall.com/cards/${set.toLowerCase()}/${encodeURIComponent(number)}?format=image&version=normal`;
}

/** The image to show for a deck entry: its chosen printing, else the default. */
export function entryImage(e: Entry): string {
  if (e.set_code && e.collector_number) return printingImage(e.set_code, e.collector_number);
  return e.card?.image_uri ?? '';
}
