import { api } from './api';
import type { CardView, Status } from './types';
import type { Currency } from './util';

export type Route =
  | { page: 'library' }
  | { page: 'deck'; name: string; tab: string }
  | { page: 'build' }
  | { page: 'settings' };

function parseHash(): Route {
  const raw = decodeURIComponent(location.hash.replace(/^#\/?/, ''));
  const [page, ...rest] = raw.split('/');
  if (page === 'deck' && rest.length) {
    const tab = rest.length > 1 ? rest[rest.length - 1] : 'cards';
    const name = rest.length > 1 ? rest.slice(0, -1).join('/') : rest[0];
    return { page: 'deck', name, tab };
  }
  if (page === 'build') return { page: 'build' };
  if (page === 'settings') return { page: 'settings' };
  return { page: 'library' };
}

export function go(route: Route) {
  let hash = '#/';
  if (route.page === 'deck') hash = `#/deck/${encodeURIComponent(route.name)}/${route.tab}`;
  else if (route.page !== 'library') hash = `#/${route.page}`;
  if (location.hash !== hash) location.hash = hash;
}

export interface Toast {
  id: number;
  text: string;
  kind: 'info' | 'error' | 'success';
}

class AppState {
  route = $state<Route>(parseHash());
  status = $state<Status | null>(null);
  toasts = $state<Toast[]>([]);
  theme = $state<'dark' | 'light' | 'system'>('dark');
  currency = $state<Currency>('usd');
  /** Card shown in the floating/side preview. */
  preview = $state<CardView | null>(null);
  /** Card opened in the detail sheet. */
  inspect = $state<CardView | null>(null);
  updatingCards = $state<{ percent: number; step: string } | null>(null);
  private toastId = 0;

  constructor() {
    window.addEventListener('hashchange', () => (this.route = parseHash()));
  }

  async load() {
    try {
      const settings = await api<Record<string, any>>('settings_get');
      if (settings.theme) this.theme = settings.theme;
      if (settings.currency) this.currency = settings.currency;
    } catch {
      /* first run */
    }
    this.applyTheme();
    await this.refreshStatus();
  }

  async refreshStatus() {
    try {
      this.status = await api<Status>('status');
    } catch (err) {
      this.toast(String((err as Error).message), 'error');
    }
  }

  applyTheme() {
    const dark =
      this.theme === 'dark' ||
      (this.theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
  }

  async saveSetting(key: string, value: unknown) {
    await api('settings_set', { settings: { [key]: value } });
  }

  toast(text: string, kind: Toast['kind'] = 'info', ms = 4200) {
    const id = ++this.toastId;
    this.toasts = [...this.toasts, { id, text, kind }];
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), ms);
  }

  error(err: unknown) {
    this.toast((err as Error)?.message ?? String(err), 'error', 7000);
  }
}

export const app = new AppState();
