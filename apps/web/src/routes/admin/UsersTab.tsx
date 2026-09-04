import { useMutation, useQueryClient } from '@tanstack/react-query';
import { format } from 'date-fns';
import { toast } from 'sonner';
import { ShieldCheck, ShieldOff } from 'lucide-react';
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

  if (isLoading) {
    return (
      <div className="flex justify-center py-20">
        <Spinner />
      </div>
    );
  }

  const users = data ?? [];
  if (users.length === 0) return <EmptyState title="No users yet" />;

  return (
    <div className="overflow-x-auto rounded-xl border border-sand bg-white">
      <table className="w-full min-w-[720px] text-sm">
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
                </td>
                <td className="px-3 py-2.5 text-xs text-muted">
                  {u.last_login_at ? format(new Date(u.last_login_at), 'MMM d, yyyy') : 'Never'}
                </td>
                <td className="px-3 py-2.5 tabular-nums text-charcoal">{u.booking_count}</td>
                <td className="px-3 py-2.5 text-right">
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
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
