/**
 * "Where was this person going?" across the login round trip.
 *
 * On a phone they leave for their email app and come back, often in a NEW
 * browser tab, where router state and sessionStorage are gone. So the target is
 * kept in router state (same-tab) and in storage (cross-tab), with a short
 * expiry so a stale target never hijacks a later, unrelated sign-in.
 *
 * Every value is run through [`safeReturnPath`] on the way in and on the way
 * out: only same-site relative paths are honoured.
 */
const KEY = 'dmc_return_to';
const TTL_MS = 30 * 60 * 1000;

/** A same-site relative path, or null. Blocks absolute URLs, `//host`,
 *  `/\host`, `javascript:` and anything with control characters. */
export function safeReturnPath(raw: unknown): string | null {
  if (typeof raw !== 'string') return null;
  const p = raw.trim();
  // eslint-disable-next-line no-control-regex
  if (!p.startsWith('/') || p.startsWith('//') || p.startsWith('/\\') || /[\u0000-\u001f\\]/.test(p)) {
    return null;
  }
  try {
    const u = new URL(p, 'http://same-site.invalid');
    if (u.origin !== 'http://same-site.invalid') return null;
    // Never bounce back into the login page itself.
    if (u.pathname === '/login') return null;
    return u.pathname + u.search + u.hash;
  } catch {
    return null;
  }
}

export function saveReturnTo(path: string): void {
  const safe = safeReturnPath(path);
  if (!safe) return;
  const value = JSON.stringify({ path: safe, at: Date.now() });
  for (const store of [() => sessionStorage, () => localStorage]) {
    try {
      store().setItem(KEY, value);
    } catch {
      /* storage unavailable — router state still covers the same-tab case */
    }
  }
}

export function clearReturnTo(): void {
  for (const store of [() => sessionStorage, () => localStorage]) {
    try {
      store().removeItem(KEY);
    } catch {
      /* ignore */
    }
  }
}

/** The stored target if present, fresh and safe. */
export function readReturnTo(): string | null {
  for (const store of [() => sessionStorage, () => localStorage]) {
    try {
      const raw = store().getItem(KEY);
      if (!raw) continue;
      const v = JSON.parse(raw) as { path?: unknown; at?: unknown };
      if (typeof v.at === 'number' && Date.now() - v.at < TTL_MS) {
        const safe = safeReturnPath(v.path);
        if (safe) return safe;
      }
    } catch {
      /* ignore */
    }
  }
  return null;
}
