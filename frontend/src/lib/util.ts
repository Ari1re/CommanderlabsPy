import type { CardView } from './types';

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

export function symbolUrl(sym: string): string {
  return `https://svgs.scryfall.io/card-symbols/${sym.replace(/\//g, '').toUpperCase()}.svg`;
}

export type Currency = 'usd' | 'eur' | 'tix';

export function price(card: CardView | null | undefined, currency: Currency): number | null {
  if (!card) return null;
  return currency === 'eur' ? card.price_eur : currency === 'tix' ? card.price_tix : card.price_usd;
}

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

export function plural(n: number, word: string, many = word + 's') {
  return `${n} ${n === 1 ? word : many}`;
}

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
