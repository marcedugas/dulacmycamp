import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import { api, getToken, setToken } from './api';
import type { AuthResponse, User } from './types';

interface AuthValue {
  user: User | null;
  /** True until the stored token has been checked against the server. */
  loading: boolean;
  isAdmin: boolean;
  signIn: (res: AuthResponse) => void;
  signOut: () => void;
  /** Re-reads the current user, e.g. after a profile save. */
  refresh: () => Promise<void>;
}

const AuthContext = createContext<AuthValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    if (!getToken()) {
      setUser(null);
      setLoading(false);
      return;
    }
    try {
      setUser(await api<User>('/auth/me'));
    } catch {
      // Expired or revoked; `api` has already cleared the token.
      setUser(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const value = useMemo<AuthValue>(
    () => ({
      user,
      loading,
      isAdmin: user?.role === 'admin',
      signIn: (res) => {
        setToken(res.token);
        setUser(res.user);
        setLoading(false);
      },
      signOut: () => {
        setToken(null);
        setUser(null);
      },
      refresh,
    }),
    [user, loading, refresh],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used inside <AuthProvider>');
  return ctx;
}
