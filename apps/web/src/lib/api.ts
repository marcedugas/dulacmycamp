/**
 * Thin fetch wrapper around the Rust API.
 *
 * The bearer token lives in localStorage so a refresh keeps you signed in;
 * it is a 24h token and the server re-reads the user on every request, so a
 * stale copy can't outlive a role change.
 */

const BASE = (import.meta.env.VITE_API_URL ?? '').replace(/\/$/, '');
const TOKEN_KEY = 'dmc_token';

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(message: string, status: number, code: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
  }
}

export function getToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

export function setToken(token: string | null) {
  try {
    if (token) localStorage.setItem(TOKEN_KEY, token);
    else localStorage.removeItem(TOKEN_KEY);
  } catch {
    /* private browsing — the session just won't survive a reload */
  }
}

interface Options {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE';
  body?: unknown;
  /** Send without a token even when one is stored. */
  anonymous?: boolean;
}

export async function api<T>(path: string, opts: Options = {}): Promise<T> {
  const { method = 'GET', body, anonymous } = opts;
  const headers: Record<string, string> = {};
  const token = anonymous ? null : getToken();

  if (token) headers.Authorization = `Bearer ${token}`;
  if (body !== undefined) headers['Content-Type'] = 'application/json';

  const res = await fetch(`${BASE}/api${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  if (!res.ok) {
    // The API always answers with { error: { code, message } }; fall back to
    // the status text if something else (a proxy, say) got in the way.
    let message = res.statusText || 'Request failed';
    let code = 'UNKNOWN';
    try {
      const payload = await res.json();
      if (payload?.error?.message) message = payload.error.message;
      if (payload?.error?.code) code = payload.error.code;
    } catch {
      /* non-JSON error body */
    }
    if (res.status === 401) setToken(null);
    throw new ApiError(message, res.status, code);
  }

  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}
