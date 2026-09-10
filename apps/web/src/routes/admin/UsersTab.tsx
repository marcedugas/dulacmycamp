import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { format } from 'date-fns';
import { toast } from 'sonner';
import { Ban, Crown, RotateCcw, Trash2 } from 'lucide-react';
import { Button, EmptyState, Modal, Spinner, cx } from '../../components/ui';
import { api, ApiError } from '../../lib/api';
import { useUsers } from '../../lib/queries';
import { useAuth } from '../../lib/auth';
import type { Role, UserWithStats } from '../../lib/types';
import { ROLE_LABELS, ROLES } from '../../lib/types';

/** Everything a hard delete would destroy. The API refuses the delete unless
 *  this is zero, so it is also what decides whether the button is offered. */
function historyCount(u: UserWithStats): number {
  return u.booking_count + u.journal_count + u.message_count;
}

export default function UsersTab() {
  const { data, isLoading } = useUsers();
  const { user: me } = useAuth();
  const queryClient = useQueryClient();

  // Blocking is reversible and gets a single click, like the role toggles.
  // Deleting is not, so it goes through this confirmation first.
  const [confirmDelete, setConfirmDelete] = useState<UserWithStats | null>(null);

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

  const remove = useMutation({
    mutationFn: (id: string) => api(`/users/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      toast.success('Account deleted.');
      setConfirmDelete(null);
      void queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: (err) =>
      toast.error(err instanceof ApiError ? err.message : 'Could not delete that account.'),
  });

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
        <table className="w-full min-w-[980px] text-sm">
          <thead className="border-b border-sand bg-cream-dark/60 text-left text-xs uppercase tracking-wide text-muted">
            <tr>
              <th className="px-3 py-2.5 font-bold">Name</th>
              <th className="px-3 py-2.5 font-bold">Email</th>
              <th className="px-3 py-2.5 font-bold">Role</th>
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
              const history = historyCount(u);
              // Both server-side rules, mirrored here so the button explains
              // itself instead of only failing when pressed.
              const blockable = !admin && !isMe;
              const deletable = blockable && history === 0;
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
                  <td className="px-3 py-2.5 text-muted">{u.email}</td>
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
                      <Button
                        size="sm"
                        variant="ghost"
                        className="text-clay"
                        // Only ever offered on an account that has nothing to
                        // lose — anything with history is a job for Block.
                        disabled={!deletable || remove.isPending}
                        title={
                          isMe
                            ? 'You cannot delete your own account'
                            : admin
                              ? 'Remove their admin role first'
                              : history > 0
                                ? 'Has booking, journal or message history — use Block instead'
                                : 'Delete this empty account for good'
                        }
                        onClick={() => setConfirmDelete(u)}
                      >
                        <Trash2 size={14} /> Delete
                      </Button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <Modal
        open={confirmDelete !== null}
        onClose={() => setConfirmDelete(null)}
        title="Delete this account?"
      >
        <div className="space-y-4">
          <p className="text-sm text-charcoal">
            <strong>{confirmDelete?.full_name ?? confirmDelete?.email}</strong> has never booked,
            posted or messaged, so there is nothing to keep. This cannot be undone, though they can
            sign up again with the same address.
          </p>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" type="button" onClick={() => setConfirmDelete(null)}>
              Cancel
            </Button>
            <Button
              variant="danger"
              type="button"
              disabled={remove.isPending}
              onClick={() => confirmDelete && remove.mutate(confirmDelete.id)}
            >
              {remove.isPending ? 'Deleting…' : 'Delete account'}
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
}
