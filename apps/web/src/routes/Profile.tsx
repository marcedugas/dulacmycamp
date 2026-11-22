import { useState } from 'react';
import { Link } from 'react-router-dom';
import { toast } from 'sonner';
import { KeyRound } from 'lucide-react';
import { Button, Card, Field, Input, PageHeader, Textarea } from '../components/ui';
import { ContactFields, contactFromUser, type ContactValues } from '../components/ContactFields';
import { api, ApiError, setToken } from '../lib/api';
import { useAuth } from '../lib/auth';
import { phoneError } from '../lib/contact';
import { MIN_PASSWORD_LEN } from '../lib/password';
import type { PasswordChanged, User } from '../lib/types';

/**
 * Optional password sign-in, available to any signed-in account.
 *
 * This never removes the emailed-code route: an account that sets a password
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
      const res = await api<PasswordChanged>('/auth/set-password', {
        method: 'POST',
        body: { password },
      });
      // Changing the password ended every older session, this one included;
      // the server sends a replacement.
      setToken(res.token);
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
          : `You can sign in with a password instead of an emailed code. Setting one doesn't turn codes off — you'll still be able to use them.`}
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

/**
 * Phone, city/state and mailing address. Saved and deleted on their own, so
 * "delete my contact info" never touches the rest of the profile.
 */
function ContactSection() {
  const { user, refresh } = useAuth();
  const [values, setValues] = useState<ContactValues>(() => contactFromUser(user));
  const [busy, setBusy] = useState(false);
  const hasAny = Boolean(user?.phone || user?.home_city || user?.home_state || user?.mailing_address);
  const bad = phoneError(values.phone);

  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    if (bad) return;
    setBusy(true);
    try {
      await api<User>('/users/me/contact', {
        method: 'PUT',
        body: {
          phone: values.phone.trim() || null,
          home_city: values.home_city.trim() || null,
          home_state: values.home_state.trim() || null,
          mailing_address: values.mailing_address.trim() || null,
        },
      });
      await refresh();
      toast.success('Contact info saved.');
    } catch (err) {
      toast.error(err instanceof ApiError ? err.message : 'Could not save your contact info.');
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!window.confirm('Delete your phone, address, city and state from your account?')) return;
    setBusy(true);
    try {
      await api<User>('/users/me/contact', { method: 'DELETE' });
      setValues({ phone: '', home_city: '', home_state: '', mailing_address: '' });
      await refresh();
      toast.success('Your contact info was deleted.');
    } catch (err) {
      toast.error(err instanceof ApiError ? err.message : 'Could not delete your contact info.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card className="mt-6">
      <h2 className="font-display text-lg font-bold text-charcoal">Contact info</h2>
      <p className="mb-4 text-sm text-muted">
        Only the camp owners can see this. We never sell it or share it with anyone else.{' '}
        <Link to="/privacy" className="font-semibold text-forest-700 underline">
          Privacy
        </Link>
      </p>
      <form onSubmit={save} className="space-y-4">
        <ContactFields values={values} onChange={setValues} errors={{ phone: bad }} stayHint />
        <div className="flex flex-wrap gap-2">
          <Button type="submit" disabled={busy || Boolean(bad)}>
            {busy ? 'Saving…' : 'Save contact info'}
          </Button>
          <Button type="button" variant="ghost" disabled={busy || !hasAny} onClick={remove}>
            Delete my contact info
          </Button>
        </div>
      </form>
    </Card>
  );
}

export default function Profile() {
  const { user, refresh } = useAuth();
  const [form, setForm] = useState({
    full_name: user?.full_name ?? '',
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

      {user && <ContactSection />}

      {user && <PasswordSection hasPassword={user.has_password} onSaved={refresh} />}
    </div>
  );
}
