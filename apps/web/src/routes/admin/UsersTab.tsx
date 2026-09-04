import { useMutation, useQueryClient } from '@tanstack/react-query';
import { format } from 'date-fns';
import { toast } from 'sonner';
import { Crown, ShieldCheck, ShieldOff } from 'lucide-react';
import { Button, EmptyState, Spinner, cx } from '../../components/ui';
import { api, ApiError } from '../../lib/api';
import { useUsers } from '../../lib/queries';
import { useAuth } from '../../lib/auth';

export default function UsersTab() {
  const { data, isLoading } = useUsers();
  const { user: me } = useAuth();
  const queryClient = useQueryClient();

  const setRole = useMutation({
    mutationFn: ({ id, role }: { id: string; role: 'guest' | 'admin' }) =>
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
        <table className="w-full min-w-[820px] text-sm">
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
              return (
                <tr key={u.id} className="border-b border-sand/70 last:border-0">
                  <td className="px-3 py-2.5">
                    <p className="font-semibold text-charcoal">{u.full_name ?? '—'}</p>
                    {u.relationship && <p className="text-xs text-muted">{u.relationship}</p>}
                  </td>
                  <td className="px-3 py-2.5 text-muted">{u.email}</td>
                  <td className="px-3 py-2.5">
                    <div className="flex flex-wrap items-center gap-1.5">
                      <span
                        className={cx(
                          'rounded-full border px-2 py-0.5 text-xs font-semibold capitalize',
                          admin
                            ? 'border-forest-300 bg-forest-100 text-forest-800'
                            : 'border-sand bg-cream-dark text-muted',
                        )}
                      >
                        {u.role}
                      </span>
                      {u.is_owner && (
                        <span
                          className="inline-flex items-center gap-1 rounded-full border border-wood-300 bg-wood-100 px-2 py-0.5 text-xs font-semibold text-wood-800"
                          title="Receives the booking approve/deny email"
                        >
                          <Crown size={11} /> Owner
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
                      <Button
                        size="sm"
                        variant="ghost"
                        // An admin can't demote themselves — the server rejects it too.
                        disabled={isMe || setRole.isPending}
                        title={isMe ? 'You cannot change your own role' : undefined}
                        onClick={() => setRole.mutate({ id: u.id, role: admin ? 'guest' : 'admin' })}
                      >
                        {admin ? (
                          <>
                            <ShieldOff size={14} /> Remove Admin
                          </>
                        ) : (
                          <>
                            <ShieldCheck size={14} /> Make Admin
                          </>
                        )}
                      </Button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
