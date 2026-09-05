import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { format } from 'date-fns';
import { toast } from 'sonner';
import { Archive, ArchiveRestore, Check, X } from 'lucide-react';
import { Button, EmptyState, Modal, Spinner, Textarea, cx } from '../../components/ui';
import { api, ApiError } from '../../lib/api';
import { useJournalAdmin } from '../../lib/queries';
import { formatRange } from '../../lib/dates';
import type { AdminJournalEntry, JournalStatus } from '../../lib/types';

const STATUSES: (JournalStatus | 'all')[] = ['all', 'pending', 'approved', 'rejected'];

const STATUS_STYLES: Record<JournalStatus, string> = {
  pending: 'bg-amber-100 text-amber-900 border-amber-300',
  approved: 'bg-forest-100 text-forest-800 border-forest-300',
  rejected: 'bg-cream-dark text-muted border-sand',
};

export default function JournalTab() {
  const [status, setStatus] = useState<JournalStatus | 'all'>('all');
  const { data, isLoading } = useJournalAdmin(status === 'all' ? undefined : status);
  const queryClient = useQueryClient();

  const [reviewing, setReviewing] = useState<AdminJournalEntry | null>(null);
  const [rejecting, setRejecting] = useState<AdminJournalEntry | null>(null);
  const [reason, setReason] = useState('');

  const invalidate = () =>
    void queryClient.invalidateQueries({ queryKey: ['journal-admin'], exact: false });
  const onError = (err: unknown) =>
    toast.error(err instanceof ApiError ? err.message : 'That action failed.');

  const approve = useMutation({
    mutationFn: (id: string) => api(`/journal/${id}/approve`, { method: 'PUT' }),
    onSuccess: () => {
      toast.success('Story approved — it’s live on the public journal.');
      setReviewing(null);
      invalidate();
    },
    onError,
  });

  const reject = useMutation({
    mutationFn: ({ id, reason }: { id: string; reason: string }) =>
      api(`/journal/${id}/reject`, { method: 'PUT', body: { reason: reason || null } }),
    onSuccess: () => {
      toast.success('Story not posted — the guest has been notified.');
      setRejecting(null);
      setReviewing(null);
      setReason('');
      invalidate();
    },
    onError,
  });

  const archive = useMutation({
    mutationFn: (id: string) => api(`/journal/${id}/archive`, { method: 'PUT' }),
    onSuccess: () => {
      toast.success('Story archived — hidden from the public feed.');
      invalidate();
    },
    onError,
  });

  const unarchive = useMutation({
    mutationFn: (id: string) => api(`/journal/${id}/unarchive`, { method: 'PUT' }),
    onSuccess: () => {
      toast.success('Story restored to the public feed.');
      invalidate();
    },
    onError,
  });

  const entries = data ?? [];

  if (isLoading) {
    return (
      <div className="flex justify-center py-20">
        <Spinner />
      </div>
    );
  }

  return (
    <>
      <div className="mb-4 flex rounded-lg border border-sand bg-white p-0.5">
        {STATUSES.map((s) => (
          <button
            key={s}
            onClick={() => setStatus(s)}
            className={cx(
              'rounded-md px-3 py-1.5 text-xs font-semibold capitalize transition',
              status === s ? 'bg-forest-600 text-cream' : 'text-muted hover:text-charcoal',
            )}
          >
            {s}
          </button>
        ))}
      </div>

      {entries.length === 0 ? (
        <EmptyState title="No journal entries here" />
      ) : (
        <div className="overflow-x-auto rounded-xl border border-sand bg-white">
          <table className="w-full min-w-[760px] text-sm">
            <thead className="border-b border-sand bg-cream-dark/60 text-left text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-3 py-2.5 font-bold">Guest</th>
                <th className="px-3 py-2.5 font-bold">Stay</th>
                <th className="px-3 py-2.5 font-bold">Title</th>
                <th className="px-3 py-2.5 font-bold">Status</th>
                <th className="px-3 py-2.5 font-bold">Submitted</th>
                <th className="px-3 py-2.5 text-right font-bold">Actions</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((e) => (
                <tr key={e.id} className="border-b border-sand/70 last:border-0">
                  <td className="px-3 py-2.5">
                    <p className="font-semibold text-charcoal">{e.guest_name ?? '—'}</p>
                    <p className="text-xs text-muted">{e.guest_email}</p>
                  </td>
                  <td className="px-3 py-2.5 text-charcoal">{formatRange(e.check_in, e.check_out)}</td>
                  <td className="max-w-[220px] truncate px-3 py-2.5 text-charcoal">{e.title}</td>
                  <td className="px-3 py-2.5">
                    <div className="flex flex-wrap gap-1">
                      <span
                        className={cx(
                          'inline-block rounded-full border px-2.5 py-0.5 text-xs font-semibold capitalize',
                          STATUS_STYLES[e.status],
                        )}
                      >
                        {e.status}
                      </span>
                      {e.archived_at && (
                        <span className="inline-block rounded-full border border-sand bg-cream-dark text-muted px-2.5 py-0.5 text-xs font-semibold">
                          Archived
                        </span>
                      )}
                    </div>
                  </td>
                  <td className="px-3 py-2.5 text-xs text-muted">
                    {format(new Date(e.created_at), 'MMM d, yyyy')}
                  </td>
                  <td className="px-3 py-2.5">
                    <div className="flex justify-end gap-1.5">
                      <Button size="sm" variant="ghost" onClick={() => setReviewing(e)}>
                        Review
                      </Button>
                      {e.status === 'approved' && !e.archived_at && (
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={archive.isPending}
                          onClick={() => archive.mutate(e.id)}
                        >
                          <Archive size={14} /> Archive
                        </Button>
                      )}
                      {e.archived_at && (
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={unarchive.isPending}
                          onClick={() => unarchive.mutate(e.id)}
                        >
                          <ArchiveRestore size={14} /> Unarchive
                        </Button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <Modal open={Boolean(reviewing)} onClose={() => setReviewing(null)} title={reviewing?.title ?? 'Story'}>
        {reviewing && (
          <div className="space-y-4">
            <div className="text-sm text-muted">
              <p>
                <strong className="text-charcoal">{reviewing.guest_name ?? reviewing.guest_email}</strong> ·{' '}
                {formatRange(reviewing.check_in, reviewing.check_out)}
              </p>
              <p>Submitted {format(new Date(reviewing.created_at), 'MMM d, yyyy')}</p>
            </div>
            <p className="max-h-64 overflow-y-auto whitespace-pre-wrap rounded-lg bg-cream-dark/50 p-3 text-sm text-charcoal">
              {reviewing.body}
            </p>
            {reviewing.rejected_reason && (
              <p className="rounded-lg bg-red-50 px-3 py-2 text-sm text-red-900">
                Rejected: {reviewing.rejected_reason}
              </p>
            )}
            {reviewing.status === 'pending' && (
              <div className="flex justify-end gap-2">
                <Button
                  variant="danger"
                  disabled={reject.isPending}
                  onClick={() => setRejecting(reviewing)}
                >
                  <X size={14} /> Reject
                </Button>
                <Button disabled={approve.isPending} onClick={() => approve.mutate(reviewing.id)}>
                  <Check size={14} /> Approve
                </Button>
              </div>
            )}
          </div>
        )}
      </Modal>

      <Modal open={Boolean(rejecting)} onClose={() => setRejecting(null)} title="Don't post this story?">
        <p className="mb-3 text-sm text-muted">
          {rejecting?.guest_name} · {rejecting?.title}
        </p>
        <Textarea
          rows={3}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          placeholder="Reason (optional — kept warm, the guest will see this)"
        />
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={() => setRejecting(null)}>
            Never mind
          </Button>
          <Button
            variant="danger"
            disabled={reject.isPending}
            onClick={() => rejecting && reject.mutate({ id: rejecting.id, reason })}
          >
            {reject.isPending ? 'Saving…' : "Don't post it"}
          </Button>
        </div>
      </Modal>
    </>
  );
}
