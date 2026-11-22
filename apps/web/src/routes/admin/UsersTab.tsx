import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { format } from 'date-fns';
import { toast } from 'sonner';
import { Ban, Copy, Crown, KeyRound, RotateCcw, Trash2 } from 'lucide-react';
import { Button, EmptyState, Modal, Spinner, cx } from '../../components/ui';
import { api, ApiError } from '../../lib/api';
import { useUsers } from '../../lib/queries';
import DeleteAccountDialog from './DeleteAccountDialog';
import { useAuth } from '../../lib/auth';
import type { Role, TempPassword, UserWithStats } from '../../lib/types';
import { ROLE_LABELS, ROLES } from '../../lib/types';

/** "No password", "Password set", or when it is an admin-issued temporary one,
 *  when it stops working — the thing an admin needs to know before helping
 *  someone over the phone. */
function passwordStatus(u: UserWithStats): { label: string; tone: 'none' | 'set' | 'temp' } {
  if (!u.has_password) return { label: 'No password', tone: 'none' };
  if (u.must_change_password) {
    const until = u.temp_password_expires_at
      ? format(new Date(u.temp_password_expires_at), 'MMM d, yyyy')
      : 'unknown';
    const expired = u.temp_password_expires_at && new Date(u.temp_password_expires_at) <= new Date();
    return { label: `Temporary — ${expired ? 'expired' : 'expires'} ${until}`, tone: 'temp' };
  }
  return { label: 'Password set', tone: 'set' };
}

export default function UsersTab() {
  const { data, isLoading } = useUsers();
  const { user: me } = useAuth();
  const queryClient = useQueryClient();

  // Blocking is reversible and gets a single click, like the role toggles.
  // Deleting is permanent: a dialog with a counts preview and a typed email.
  const [confirmDelete, setConfirmDelete] = useState<UserWithStats | null>(null);
  // Password tools: which account a confirmation is open for, and the one
  // temporary password currently on screen. The latter lives only in this
  // state — closing the dialog drops it, and nothing else ever holds it.
  const [confirmTemp, setConfirmTemp] = useState<UserWithStats | null>(null);
  const [confirmRemovePw, setConfirmRemovePw] = useState<UserWithStats | null>(null);
  const [shownTemp, setShownTemp] = useState<{ who: string; temp: TempPassword } | null>(null);

  const setRole = useMutation({
    mutationFn: ({ id, role }: { id: string; role: Role }) =>
      api(`/users/${id}/role`, { method: 'PUT', body: { role } }),
    onSuccess: () => {
      toast.success('Role updated.');
      void queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: (err) =>
      toast.error(err instanceof ApiError ? err.message : 'Could not change that role.'),
  });

  const setOwner = useMutation({
    mutationFn: ({ id, is_owner }: { id: string; is_owner: boolean }) =>
      api(`/users/${id}/owner`, { method: 'PUT', body: { is_owner } }),
    onSuccess: (_data, { is_owner }) => {
      toast.success(
        is_owner
          ? 'Added to the booking approval emails.'
          : 'Removed from the booking approval emails.',
      );
      void queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: (err) =>
      toast.error(err instanceof ApiError ? err.message : 'Could not change that owner flag.'),
  });

  const clearContact = useMutation({
    mutationFn: (id: string) => api(`/users/${id}/contact`, { method: 'DELETE' }),
    onSuccess: () => {
      toast.success('Contact info cleared.');
      void queryClient.invalidateQueries({ queryKey: ['users'] });
      void queryClient.invalidateQueries({ queryKey: ['bookings'] });
    },
    onError: (err) =>
      toast.error(err instanceof ApiError ? err.message : 'Could not clear that contact info.'),
  });

  const setBlocked = useMutation({
    mutationFn: ({ id, blocked }: { id: string; blocked: boolean }) =>
      api(`/users/${id}/${blocked ? 'block' : 'unblock'}`, { method: 'PUT' }),
    onSuccess: (_data, { blocked }) => {
      toast.success(
        blocked
          ? 'Blocked. Their bookings are untouched — cancel any you also want called off.'
          : 'Unblocked. They can sign in again.',
      );
      void queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: (err) =>
      toast.error(err instanceof ApiError ? err.message : 'Could not change that account.'),
  });

  const setTempPassword = useMutation({
    mutationFn: (u: UserWithStats) => api<TempPassword>(`/users/${u.id}/temp-password`, { method: 'POST' }),
    onSuccess: (temp, u) => {
      setConfirmTemp(null);
      setShownTemp({ who: u.full_name ?? u.email, temp });
      void queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: (err) =>
      toast.error(err instanceof ApiError ? err.message : 'Could not set a temporary password.'),
  });

  const removePassword = useMutation({
    mutationFn: (u: UserWithStats) => api(`/users/${u.id}/remove-password`, { method: 'POST' }),
    onSuccess: () => {
      toast.success('Password removed. They can sign in with an emailed code.');
      setConfirmRemovePw(null);
      void queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: (err) =>
      toast.error(err instanceof ApiError ? err.message : 'Could not remove that password.'),
  });

  const copyTemp = async () => {
    if (!shownTemp) return;
    try {
      await navigator.clipboard.writeText(shownTemp.temp.password);
      toast.success('Copied.');
    } catch {
      toast.error('Could not copy. Select the password and copy it by hand.');
    }
  };

  if (isLoading) {
    return (
      <div className="flex justify-center py-20">
        <Spinner />
      </div>
    );
  }

  const users = data ?? [];
  if (users.length === 0) return <EmptyState title="No users yet" />;

  const owners = users.filter((u) => u.is_owner).length;

  return (
    <div className="space-y-3">
      {/* The send path falls back to OWNER_EMAIL here, but silently as far as
          the admin can see — so say it in the UI rather than only in the log. */}
      {owners === 0 && (
        <p className="rounded-lg border border-clay/30 bg-clay/5 px-3 py-2 text-xs text-clay">
          No one is flagged as an owner, so booking approvals fall back to the{' '}
          <code>OWNER_EMAIL</code> setting. Flag whoever should approve stays.
        </p>
      )}

      <div className="overflow-x-auto rounded-xl border border-sand bg-white">
        <table className="w-full min-w-[1100px] text-sm">
          <thead className="border-b border-sand bg-cream-dark/60 text-left text-xs uppercase tracking-wide text-muted">
            <tr>
              <th className="px-3 py-2.5 font-bold">Name</th>
              <th className="px-3 py-2.5 font-bold">Email</th>
              <th className="px-3 py-2.5 font-bold">Role</th>
              <th className="px-3 py-2.5 font-bold">Password</th>
              <th className="px-3 py-2.5 font-bold">Last login</th>
              <th className="px-3 py-2.5 font-bold">Bookings</th>
              <th className="px-3 py-2.5 text-right font-bold">Actions</th>
            </tr>
          </thead>
          <tbody>
            {users.map((u) => {
              const isMe = u.id === me?.id;
              const admin = u.role === 'admin';
              const blocked = u.blocked_at !== null;
              const pw = passwordStatus(u);
              // Mirrors the server rule so the button explains itself instead
              // of only failing when pressed.
              const blockable = !admin && !isMe;
              // Delete is not offered at all for yourself, admins or owners:
              // demote first, then delete.
              const deletable = blockable && !u.is_owner;
              return (
                <tr
                  key={u.id}
                  className={cx(
                    'border-b border-sand/70 last:border-0',
                    blocked && 'bg-clay/5 text-muted',
                  )}
                >
                  <td className="px-3 py-2.5">
                    <p
                      className={cx(
                        'font-semibold',
                        blocked ? 'text-muted line-through' : 'text-charcoal',
                      )}
                    >
                      {u.full_name ?? '—'}
                    </p>
                    {u.relationship && <p className="text-xs text-muted">{u.relationship}</p>}
                  </td>
                  <td className="px-3 py-2.5 text-muted">
                    {u.email}
                    {(u.phone || u.home_city || u.mailing_address) && (
                      <div className="mt-1 text-xs">
                        {u.phone && (
                          <a href={`tel:${u.phone}`} className="font-semibold text-forest-700 underline">
                            {u.phone}
                          </a>
                        )}
                        {(u.home_city || u.home_state) && (
                          <span> · {[u.home_city, u.home_state].filter(Boolean).join(', ')}</span>
                        )}
                        {u.mailing_address && <div className="whitespace-pre-line">{u.mailing_address}</div>}
                        <button
                          type="button"
                          className="mt-0.5 block text-clay underline"
                          disabled={clearContact.isPending}
                          onClick={() => {
                            if (window.confirm('Clear this person\'s phone, address, city and state?'))
                              clearContact.mutate(u.id);
                          }}
                        >
                          Clear contact info
                        </button>
                      </div>
                    )}
                  </td>
                  <td className="px-3 py-2.5">
                    <div className="flex flex-wrap items-center gap-1.5">
                      <span
                        className={cx(
                          'rounded-full border px-2 py-0.5 text-xs font-semibold',
                          admin
                            ? 'border-forest-300 bg-forest-100 text-forest-800'
                            : u.role === 'user'
                              ? 'border-bayou-300 bg-bayou-100 text-bayou-800'
                              : 'border-sand bg-cream-dark text-muted',
                        )}
                      >
                        {ROLE_LABELS[u.role as Role] ?? u.role}
                      </span>
                      {u.is_owner && (
                        <span
                          className="inline-flex items-center gap-1 rounded-full border border-wood-300 bg-wood-100 px-2 py-0.5 text-xs font-semibold text-wood-800"
                          title="Receives the booking approve/deny email"
                        >
                          <Crown size={11} /> Owner
                        </span>
                      )}
                      {blocked && (
                        <span
                          className="inline-flex items-center gap-1 rounded-full border border-clay/40 bg-clay/10 px-2 py-0.5 text-xs font-semibold text-clay"
                          title={`Blocked ${format(new Date(u.blocked_at!), 'MMM d, yyyy')} — cannot sign in`}
                        >
                          <Ban size={11} /> Blocked
                        </span>
                      )}
                    </div>
                  </td>
                  <td className="px-3 py-2.5 text-xs">
                    <span
                      className={cx(
                        'font-semibold',
                        pw.tone === 'temp' ? 'text-clay' : pw.tone === 'set' ? 'text-charcoal' : 'text-muted',
                      )}
                    >
                      {pw.label}
                    </span>
                  </td>
                  <td className="px-3 py-2.5 text-xs text-muted">
                    {u.last_login_at ? format(new Date(u.last_login_at), 'MMM d, yyyy') : 'Never'}
                  </td>
                  <td className="px-3 py-2.5 tabular-nums text-charcoal">{u.booking_count}</td>
                  <td className="px-3 py-2.5">
                    <div className="flex justify-end gap-1">
                      <Button
                        size="sm"
                        variant="ghost"
                        // Any number of owners is allowed, including none —
                        // the server doesn't restrict this either.
                        disabled={setOwner.isPending}
                        title={
                          u.is_owner
                            ? 'Stop sending them booking approvals'
                            : 'Send them the booking approve/deny email'
                        }
                        onClick={() => setOwner.mutate({ id: u.id, is_owner: !u.is_owner })}
                      >
                        <Crown size={14} /> {u.is_owner ? 'Remove Owner' : 'Make Owner'}
                      </Button>
                      {/* Three tiers no longer fit an on/off button. Family
                          ("user") is a sideways move from guest, not a step
                          toward admin, so nothing here implies an order. */}
                      <select
                        aria-label={`Role for ${u.full_name ?? u.email}`}
                        value={u.role}
                        // An admin can't demote themselves — the server rejects it too.
                        disabled={isMe || setRole.isPending}
                        title={
                          isMe
                            ? 'You cannot change your own role'
                            : 'Family accounts need no booking to keep their access'
                        }
                        onChange={(e) => setRole.mutate({ id: u.id, role: e.target.value as Role })}
                        className="rounded-lg border border-sand bg-white px-2 py-1.5 text-xs font-semibold text-charcoal disabled:opacity-50"
                      >
                        {ROLES.map((r) => (
                          <option key={r} value={r}>
                            {ROLE_LABELS[r]}
                          </option>
                        ))}
                      </select>
                      <Button
                        size="sm"
                        variant="ghost"
                        // Blocking an admin would let one lock the panel that
                        // undoes it, so the server refuses; demote them first.
                        disabled={(!blocked && !blockable) || setBlocked.isPending}
                        title={
                          blocked
                            ? 'Let them sign in again'
                            : isMe
                              ? 'You cannot block yourself'
                              : admin
                                ? 'Remove their admin role first, then block them'
                                : 'Stop them signing in. Their bookings and history stay.'
                        }
                        onClick={() => setBlocked.mutate({ id: u.id, blocked: !blocked })}
                      >
                        {blocked ? (
                          <>
                            <RotateCcw size={14} /> Unblock
                          </>
                        ) : (
                          <>
                            <Ban size={14} /> Block
                          </>
                        )}
                      </Button>
                      {/* Not offered on your own row — the server refuses it
                          too; you change your own password on your profile. */}
                      {!isMe && (
                        <>
                          <Button
                            size="sm"
                            variant="ghost"
                            disabled={setTempPassword.isPending}
                            title="Generate a one-time password to give them by phone"
                            onClick={() => setConfirmTemp(u)}
                          >
                            <KeyRound size={14} /> Set temporary password
                          </Button>
                          <Button
                            size="sm"
                            variant="ghost"
                            disabled={!u.has_password || removePassword.isPending}
                            title={
                              u.has_password
                                ? 'Clear their password so they can only use emailed codes'
                                : 'They have no password to remove'
                            }
                            onClick={() => setConfirmRemovePw(u)}
                          >
                            Remove password
                          </Button>
                        </>
                      )}
                      {deletable && (
                        <Button
                          size="sm"
                          variant="ghost"
                          className="text-clay"
                          title="Permanently delete this account and everything about it"
                          onClick={() => setConfirmDelete(u)}
                        >
                          <Trash2 size={14} /> Delete account…
                        </Button>
                      )}
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <Modal
        open={confirmTemp !== null}
        onClose={() => setConfirmTemp(null)}
        title="Set a temporary password?"
      >
        <div className="space-y-4">
          <p className="text-sm text-charcoal">
            This replaces <strong>{confirmTemp?.full_name ?? confirmTemp?.email}</strong>&apos;s
            password with a one-time one that works for 7 days. They must choose a new password the
            first time they use it, and every device they are signed in on is signed out. They will
            be emailed that an administrator did this, and it is logged. You will see the password
            once.
          </p>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" type="button" onClick={() => setConfirmTemp(null)}>
              Cancel
            </Button>
            <Button
              type="button"
              disabled={setTempPassword.isPending}
              onClick={() => confirmTemp && setTempPassword.mutate(confirmTemp)}
            >
              {setTempPassword.isPending ? 'Setting…' : 'Set temporary password'}
            </Button>
          </div>
        </div>
      </Modal>

      <Modal
        open={confirmRemovePw !== null}
        onClose={() => setConfirmRemovePw(null)}
        title="Remove this password?"
      >
        <div className="space-y-4">
          <p className="text-sm text-charcoal">
            <strong>{confirmRemovePw?.full_name ?? confirmRemovePw?.email}</strong> will no longer
            have a password and every device they are signed in on is signed out. They can still
            sign in with an emailed code, and set a new password afterwards. They will be emailed
            that an administrator did this, and it is logged.
          </p>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" type="button" onClick={() => setConfirmRemovePw(null)}>
              Cancel
            </Button>
            <Button
              variant="danger"
              type="button"
              disabled={removePassword.isPending}
              onClick={() => confirmRemovePw && removePassword.mutate(confirmRemovePw)}
            >
              {removePassword.isPending ? 'Removing…' : 'Remove password'}
            </Button>
          </div>
        </div>
      </Modal>

      {/* Closing this drops the only copy there is. */}
      <Modal
        open={shownTemp !== null}
        onClose={() => setShownTemp(null)}
        title="Temporary password"
      >
        <div className="space-y-4">
          <p className="text-sm text-charcoal">
            Temporary password for <strong>{shownTemp?.who}</strong>. It works until{' '}
            {shownTemp ? format(new Date(shownTemp.temp.expires_at), 'MMM d, yyyy') : ''}.
          </p>
          <div className="flex items-center gap-2">
            <code
              data-testid="temp-password"
              className="flex-1 select-all break-all rounded-lg border border-sand bg-white px-3 py-2.5 font-mono text-lg tracking-wide text-charcoal"
            >
              {shownTemp?.temp.password}
            </code>
            <Button type="button" variant="ghost" onClick={() => void copyTemp()}>
              <Copy size={14} /> Copy
            </Button>
          </div>
          <p className="rounded-lg border border-clay/30 bg-clay/5 px-3 py-2 text-sm text-clay">
            This is shown once. Give it to them by phone or in person, not by text or email.
          </p>
          <div className="flex justify-end">
            <Button type="button" onClick={() => setShownTemp(null)}>
              Done
            </Button>
          </div>
        </div>
      </Modal>

      <DeleteAccountDialog user={confirmDelete} onClose={() => setConfirmDelete(null)} />
    </div>
  );
}
