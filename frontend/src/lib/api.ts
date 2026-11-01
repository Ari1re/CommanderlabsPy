// One bridge for every platform: inside the Tauri app (desktop/Android) calls
// go through `invoke`, in the browser edition they go to POST /api/<cmd>.

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

let tauriInvoke: Invoke | null = null;
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function getInvoke(): Promise<Invoke | null> {
  if (!isTauri) return null;
  if (!tauriInvoke) {
    const mod = await import('@tauri-apps/api/core');
    tauriInvoke = mod.invoke as Invoke;
  }
  return tauriInvoke;
}

export const runtime = {
  isTauri,
  isAndroid: typeof navigator !== 'undefined' && /android/i.test(navigator.userAgent),
};

export class ApiError extends Error {}

/** Open a web address outside the app: the system browser in the desktop and
 * Android apps (on Android an .apk link hands over to the installer), a new tab
 * in the browser edition. */
export async function openExternal(url: string) {
  const invoke = await getInvoke();
  if (invoke) {
    try {
      await invoke('plugin:opener|open_url', { url });
      return;
    } catch {
      /* fall through */
    }
  }
  window.open(url, '_blank', 'noopener');
}

/** Call one of the native app's own commands (desktop/Android app only). */
export async function invokeNative<T = any>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const invoke = await getInvoke();
  if (!invoke) throw new ApiError('This only works in the Commander Lab app.');
  try {
    return (await invoke(cmd, args)) as T;
  } catch (err) {
    throw new ApiError(typeof err === 'string' ? err : (err as Error)?.message ?? String(err));
  }
}

export async function api<T = any>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const invoke = await getInvoke();
  if (invoke) {
    try {
      return (await invoke('api', { cmd, args })) as T;
    } catch (err) {
      throw new ApiError(typeof err === 'string' ? err : (err as Error)?.message ?? String(err));
    }
  }
  const res = await fetch(`/api/${cmd}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(args),
  });
  if (!res.ok) throw new ApiError(`Commander Lab server returned ${res.status}`);
  const body = await res.json();
  if (!body.ok) throw new ApiError(body.error ?? 'Something went wrong');
  return body.data as T;
}

export interface Job {
  id: number;
  kind: string;
  subject: string;
  status: 'running' | 'done' | 'error';
  percent: number;
  step: string;
  detail: string;
  log: string[];
  /** Text produced so far (an AI answer as it's written). */
  partial?: string;
  result: any;
  error: string | null;
}

/** Start a job command, then poll until it finishes. */
export async function runJob<T = any>(
  cmd: string,
  args: Record<string, unknown>,
  onProgress?: (job: Job) => void,
  intervalMs = 600,
): Promise<T> {
  const { job: id } = await api<{ job: number }>(cmd, args);
  return waitJob<T>(id, onProgress, intervalMs);
}

export async function waitJob<T = any>(id: number, onProgress?: (job: Job) => void, intervalMs = 600): Promise<T> {
  for (;;) {
    const job = await api<Job>('job', { id });
    onProgress?.(job);
    if (job.status === 'done') return job.result as T;
    if (job.status === 'error') throw new ApiError(job.error ?? 'The task failed');
    await new Promise((r) => setTimeout(r, intervalMs));
  }
}

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const area = document.createElement('textarea');
      area.value = text;
      area.style.position = 'fixed';
      area.style.opacity = '0';
      document.body.appendChild(area);
      area.select();
      const ok = document.execCommand('copy');
      area.remove();
      return ok;
    } catch {
      return false;
    }
  }
}

/** Browser download (web edition). In the app, files are already saved to exports/. */
export function download(filename: string, text: string, type = 'text/plain') {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
