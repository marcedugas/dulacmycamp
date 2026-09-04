import { useState } from 'react';
import { toast } from 'sonner';
import { Button, Card, Field, Input, PageHeader, Textarea } from '../components/ui';
import { api, ApiError } from '../lib/api';
import { useAuth } from '../lib/auth';
import type { User } from '../lib/types';

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
    </div>
  );
}
