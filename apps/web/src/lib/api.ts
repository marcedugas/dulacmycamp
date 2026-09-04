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

/**
 * Resolves a server-relative path (an uploaded image's `/uploads/...` URL)
 * against the API's own origin. The web and API services sit on different
 * subdomains in production, so a bare relative path would 404 there even
 * though it works in local dev (the Vite proxy papers over it).
 */
export function assetUrl(path: string | null | undefined): string | null {
  if (!path) return null;
  if (/^https?:\/\//.test(path)) return path;
  return `${BASE}${path}`;
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
  // FormData (file uploads) must NOT get a Content-Type set here — the
  // browser needs to add its own boundary parameter, which it only does
  // when the header is left unset.
  const isFormData = body instanceof FormData;

  if (token) headers.Authorization = `Bearer ${token}`;
  if (body !== undefined && !isFormData) headers['Content-Type'] = 'application/json';

  let requestBody: BodyInit | undefined;
  if (body === undefined) requestBody = undefined;
  else if (body instanceof FormData) requestBody = body;
  else requestBody = JSON.stringify(body);

  const res = await fetch(`${BASE}/api${path}`, { method, headers, body: requestBody });

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
