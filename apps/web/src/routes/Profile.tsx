import { useState } from 'react';
import { toast } from 'sonner';
import { KeyRound } from 'lucide-react';
import { Button, Card, Field, Input, PageHeader, Textarea } from '../components/ui';
import { api, ApiError } from '../lib/api';
import { useAuth } from '../lib/auth';
import type { MessageResponse, User } from '../lib/types';

/** Mirrors `password::MIN_PASSWORD_LEN`; the server is still the authority. */
const MIN_PASSWORD_LEN = 10;

/**
 * Optional password sign-in, for admins only — rendered nowhere else, and
 * refused by the server for anyone else regardless.
 *
 * This never removes the emailed-code route: an admin who sets a password
 * keeps both, which is also what makes a forgotten password a non-event.
 */
function PasswordSection({ hasPassword, onSaved }: { hasPassword: boolean; onSaved: () => Promise<void> }) {
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [busy, setBusy] = useState(false);

  const tooShort = password.length > 0 && password.length < MIN_PASSWORD_LEN;
  const mismatch = confirm.length > 0 && confirm !== password;
  const valid = password.length >= MIN_PASSWORD_LEN && confirm === password;

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      const res = await api<MessageResponse>('/auth/set-password', {
        method: 'POST',
        body: { password },
      });
      setPassword('');
      setConfirm('');
      await onSaved();
      toast.success(res.message);
    } catch (err) {
      toast.error(err instanceof ApiError ? err.message : 'Could not save that password.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card className="mt-6">
      <div className="mb-1 flex items-center gap-2">
        <KeyRound size={16} className="text-muted" />
        <h2 className="font-display text-lg font-bold text-charcoal">
          {hasPassword ? 'Change password' : 'Set a password'}
        </h2>
      </div>
      <p className="mb-4 text-sm text-muted">
        {hasPassword
          ? 'You can sign in with this password instead of waiting for a code. Emailed codes keep working either way.'
          : `Admin accounts can sign in with a password instead of an emailed code. Setting one doesn't turn codes off — you'll still be able to use them.`}
      </p>

      <form onSubmit={submit} className="space-y-4">
        <Field label="New password" hint={`At least ${MIN_PASSWORD_LEN} characters. A short phrase works well.`}>
          <Input
            type="password"
            autoComplete="new-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </Field>
        <Field label="Confirm new password">
          <Input
            type="password"
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

        <Button type="submit" disabled={busy || !valid}>
          {busy ? 'Saving…' : hasPassword ? 'Change password' : 'Set password'}
        </Button>
      </form>
    </Card>
  );
}

export default function Profile() {
  const { user, refresh } = useAuth();
  const [form, setForm] = useState({
    full_name: user?.full_name ?? '',
    phone: user?.phone ?? '',
    relationship: user?.relationship ?? '',
    boat_info: user?.boat_info ?? '',
    notes: user?.notes ?? '',
  });
  const [busy, setBusy] = useState(false);

  const set = (key: keyof typeof form) => (e: { target: { value: string } }) =>
    setForm((f) => ({ ...f, [key]: e.target.value }));

  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      await api<User>('/users/me', { method: 'PUT', body: form });
      await refresh();
      toast.success('Profile saved.');
    } catch (err) {
      toast.error(err instanceof ApiError ? err.message : 'Could not save your profile.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto max-w-2xl px-4 py-10">
      <PageHeader
        title="Your profile"
        subtitle={user?.email}
      />

      <Card>
        <form onSubmit={save} className="space-y-5">
          <Field label="Full name">
            <Input required value={form.full_name} onChange={set('full_name')} placeholder="Jean Dugas" />
          </Field>

          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Phone">
              <Input type="tel" value={form.phone} onChange={set('phone')} placeholder="(985) 555-0134" />
            </Field>
            <Field label="Relationship to the owners" hint="Cousin, friend of Marc, etc.">
              <Input value={form.relationship} onChange={set('relationship')} />
            </Field>
          </div>

          <Field label="Boat info" hint="Length, draft, whether you need the slip or the trailer spot.">
            <Input value={form.boat_info} onChange={set('boat_info')} placeholder="21' bay boat, shallow draft" />
          </Field>

          <Field label="Notes" hint="Anything the owners should know.">
            <Textarea rows={3} value={form.notes} onChange={set('notes')} />
          </Field>

          <Button type="submit" size="lg" disabled={busy}>
            {busy ? 'Saving…' : 'Save Profile'}
          </Button>
        </form>
      </Card>

      {user?.role === 'admin' && (
        <PasswordSection hasPassword={user.has_password} onSaved={refresh} />
      )}
    </div>
  );
}
