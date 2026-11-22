import { useState } from 'react';
import { toast } from 'sonner';
import { ShieldAlert } from 'lucide-react';
import { api, ApiError, setToken } from '../lib/api';
import { useAuth } from '../lib/auth';
import { MIN_PASSWORD_LEN, PASSWORD_RULES } from '../lib/password';
import type { PasswordChanged } from '../lib/types';
import { Button, Card, Field } from '../components/ui';
import { PasswordInput } from '../components/PasswordInput';

/**
 * Shown, full page and before anything else, to an account whose password is a
 * temporary one an admin issued. The server refuses every other route until
 * this succeeds; this screen is just the friendly way through.
 *
 * Rendered in place of the whole app by `App`, so on success the person is
 * already standing at the page they were headed for — nothing to navigate.
 */
export default function ChoosePassword() {
  const { user, refresh, signOut } = useAuth();
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [busy, setBusy] = useState(false);

  const tooShort = password.length > 0 && password.length < MIN_PASSWORD_LEN;
  const mismatch = confirm.length > 0 && confirm !== password;
  const ready = password.length >= MIN_PASSWORD_LEN && confirm === password;

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      const res = await api<PasswordChanged>('/auth/set-password', {
        method: 'POST',
        body: { password },
      });
      setToken(res.token);
      await refresh();
      toast.success('Password changed.');
    } catch (err) {
      toast.error(err instanceof ApiError ? err.message : 'Could not save that password.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto flex min-h-screen max-w-md flex-col justify-center px-4 py-16">
      <div className="mb-6 text-center">
        <ShieldAlert className="mx-auto mb-3 text-forest-600" size={34} aria-hidden />
        <h1 className="text-2xl font-bold text-charcoal">Choose a new password</h1>
        <p className="mt-1 text-sm text-muted">
          You signed in with a temporary password{user ? ` (${user.email})` : ''}. Pick one of your
          own to continue.
        </p>
      </div>
      <Card>
        <form onSubmit={submit} className="space-y-4">
          <Field label="New password" hint={PASSWORD_RULES}>
            <PasswordInput
              aria-label="New password"
              required
              autoFocus
              autoComplete="new-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </Field>
          <Field label="Confirm new password">
            <PasswordInput
              aria-label="Confirm new password"
              required
              autoComplete="new-password"
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
            />
          </Field>
          {tooShort && (
            <p className="text-sm text-clay">
              That&apos;s {password.length} characters — {MIN_PASSWORD_LEN} is the minimum.
            </p>
          )}
          {mismatch && <p className="text-sm text-clay">Those two don&apos;t match.</p>}
          <Button type="submit" size="lg" className="w-full" disabled={busy || !ready}>
            {busy ? 'Saving…' : 'Save new password'}
          </Button>
        </form>
      </Card>
      <button
        type="button"
        onClick={signOut}
        className="mt-4 text-center text-sm font-semibold text-muted hover:text-charcoal"
      >
        Sign out
      </button>
    </div>
  );
}
