import { useCallback, useRef, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { format } from 'date-fns';
import { toast } from 'sonner';
import { Archive, ArchiveRestore, Pencil, Trash2, TriangleAlert } from 'lucide-react';
import { Button, EmptyState, JournalVisibilityBadge, Modal, Spinner, cx } from '../../components/ui';
import { CatchSummary } from '../../components/CatchLog';
import { JournalComposer } from '../../components/JournalComposer';
import { api, ApiError } from '../../lib/api';
import { useJournalAdmin } from '../../lib/queries';
import { formatRange } from '../../lib/dates';
import { confirmDiscard } from '../../lib/useLeaveGuard';
import type { AdminJournalEntry, JournalEntry } from '../../lib/types';

const onError = (err: unknown) =>
  toast.error(err instanceof ApiError ? err.message : 'That action failed.');

/**
 * After-the-fact moderation, which is all there is now — entries publish
 * themselves, so nothing here is a gate. Three levers, escalating: edit the
 * content, hide it (archive, reversible), or delete it outright.
 */

/**
 * Every photo an entry would lose to a delete: the gallery's, plus the ones
 * attached to individual catches, which the API sends inside those rows
 * rather than in `photos`.
 */
const photoCount = (e: { photos: unknown[]; catches: { photos: unknown[] }[] }) =>
  e.photos.length + e.catches.reduce((n, c) => n + c.photos.length, 0);

export default function JournalTab() {
  const { data, isLoading } = useJournalAdmin();
  const queryClient = useQueryClient();

  const [editing, setEditing] = useState<AdminJournalEntry | null>(null);
  const [deleting, setDeleting] = useState<AdminJournalEntry | null>(null);

  // Whether closing the editor now would throw away unsaved work. A ref, so
  // the modal's close handler reads it live rather than a render behind — a
  // quick Escape straight after typing must still ask.
  const editorDirty = useRef(false);
  const trackDirty = useCallback((dirty: boolean) => {
    editorDirty.current = dirty;
  }, []);

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ['journal-admin'] });
    void queryClient.invalidateQueries({ queryKey: ['journal-feed'], exact: false });
    void queryClient.invalidateQueries({ queryKey: ['journal-mine'] });
  };

  const closeEditor = () => {
    setEditing(null);
    editorDirty.current = false;
  };

  const archive = useMutation({
    mutationFn: (id: string) => api(`/journal/${id}/archive`, { method: 'PUT' }),
    onSuccess: () => {
      toast.success('Entry hidden — only its author and moderators can see it now.');
      invalidate();
    },
    onError,
  });

  const unarchive = useMutation({
    mutationFn: (id: string) => api(`/journal/${id}/unarchive`, { method: 'PUT' }),
    onSuccess: () => {
      toast.success('Entry restored to the journal.');
      invalidate();
    },
    onError,
  });

  const remove = useMutation({
    mutationFn: (id: string) =>
      api<{ deleted: boolean; photos_removed: number }>(`/journal/${id}`, { method: 'DELETE' }),
    onSuccess: (res) => {
      toast.success(
        res.photos_removed > 0
          ? `Entry deleted, along with ${res.photos_removed} photo${res.photos_removed === 1 ? '' : 's'}.`
          : 'Entry deleted.',
      );
      setDeleting(null);
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
      <p className="mb-4 text-sm text-muted">
        Entries go live as soon as they're written — there's no review queue. Use these to fix,
        hide, or remove something after the fact.
      </p>

      {entries.length === 0 ? (
        <EmptyState title="No journal entries yet" />
      ) : (
        <div className="overflow-x-auto rounded-xl border border-sand bg-white">
          <table className="w-full min-w-[860px] text-sm">
            <thead className="border-b border-sand bg-cream-dark/60 text-left text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-3 py-2.5 font-bold">Guest</th>
                <th className="px-3 py-2.5 font-bold">Stay</th>
                <th className="px-3 py-2.5 font-bold">Entry</th>
                <th className="px-3 py-2.5 font-bold">Visible to</th>
                <th className="px-3 py-2.5 font-bold">Posted</th>
                <th className="px-3 py-2.5 text-right font-bold">Actions</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((e) => (
                <tr
                  key={e.id}
                  className={cx(
                    'border-b border-sand/70 last:border-0',
                    e.archived_at && 'bg-cream-dark/40',
                  )}
                >
                  <td className="px-3 py-2.5">
                    <p className="font-semibold text-charcoal">{e.guest_name ?? '—'}</p>
                    <p className="text-xs text-muted">{e.guest_email}</p>
                  </td>
                  <td className="px-3 py-2.5 text-charcoal">
                    {formatRange(e.check_in, e.check_out)}
                  </td>
                  <td className="max-w-[260px] px-3 py-2.5">
                    <p className="truncate text-charcoal">{e.title}</p>
                    <p className="text-xs text-muted">
                      {[
                        e.catches.length > 0 &&
                          `${e.catches.length} catch${e.catches.length === 1 ? '' : 'es'}`,
                        photoCount(e) > 0 &&
                          `${photoCount(e)} photo${photoCount(e) === 1 ? '' : 's'}`,
                      ]
                        .filter(Boolean)
                        .join(' · ')}
                    </p>
                  </td>
                  <td className="px-3 py-2.5">
                    <JournalVisibilityBadge
                      visibility={e.visibility}
                      archived={Boolean(e.archived_at)}
                    />
                  </td>
                  <td className="px-3 py-2.5 text-xs text-muted">
                    {format(new Date(e.created_at), 'MMM d, yyyy')}
                  </td>
                  <td className="px-3 py-2.5">
                    <div className="flex justify-end gap-1.5">
                      <Button size="sm" variant="ghost" onClick={() => setEditing(e)}>
                        <Pencil size={14} /> Edit
                      </Button>
                      {e.archived_at ? (
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={unarchive.isPending}
                          onClick={() => unarchive.mutate(e.id)}
                        >
                          <ArchiveRestore size={14} /> Unhide
                        </Button>
                      ) : (
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={archive.isPending}
                          onClick={() => archive.mutate(e.id)}
                        >
                          <Archive size={14} /> Hide
                        </Button>
                      )}
                      <button
                        onClick={() => setDeleting(e)}
                        title="Delete permanently"
                        aria-label="Delete permanently"
                        className="shrink-0 rounded p-2 text-muted hover:bg-cream-dark hover:text-clay"
                      >
                        <Trash2 size={16} />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* The ✕, Escape and a backdrop click are easy to hit by accident, so
          they ask first when there's work to lose. The form's own Cancel is
          deliberate and doesn't. */}
      <Modal
        open={Boolean(editing)}
        onClose={() => (!editorDirty.current || confirmDiscard()) && closeEditor()}
        title={`Edit: ${editing?.title ?? ''}`}
        size="lg"
      >
        {editing && (
          <div className="space-y-4">
            <p className="text-sm text-muted">
              <strong className="text-charcoal">{editing.guest_name ?? editing.guest_email}</strong>{' '}
              · {formatRange(editing.check_in, editing.check_out)}
            </p>
            <JournalComposer
              key={editing.id}
              source={editing}
              save={(payload) =>
                api<JournalEntry>(`/journal/${editing.id}`, { method: 'PUT', body: payload })
              }
              submitLabel="Save Changes"
              submittingLabel="Saving…"
              onDone={({ skippedPhotos }) => {
                toast.success(
                  skippedPhotos > 0 ? 'Entry updated — some photos weren\'t attached.' : 'Entry updated.',
                );
                closeEditor();
                invalidate();
              }}
              onCancel={closeEditor}
              onDirtyChange={trackDirty}
              visibilityName="admin-journal-visibility"
            />
          </div>
        )}
      </Modal>

      <Modal
        open={Boolean(deleting)}
        onClose={() => setDeleting(null)}
        title="Delete this entry permanently?"
      >
        {deleting && (
          <div className="space-y-4">
            <p className="flex items-start gap-2 rounded-lg border border-red-300 bg-red-50 px-3 py-2.5 text-sm text-red-900">
              <TriangleAlert size={16} className="mt-0.5 shrink-0" />
              <span>
                This can't be undone. The story, its {deleting.catches.length} catch rows and{' '}
                {photoCount(deleting)} photos are removed for good — photo files included. To just
                take it off the journal, hide it instead.
              </span>
            </p>
            <div className="text-sm text-muted">
              <p>
                <strong className="text-charcoal">{deleting.guest_name ?? deleting.guest_email}</strong>{' '}
                · {deleting.title}
              </p>
              <CatchSummary catches={deleting.catches} />
            </div>
            <div className="flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setDeleting(null)}>
                Never mind
              </Button>
              <Button
                variant="danger"
                disabled={remove.isPending}
                onClick={() => remove.mutate(deleting.id)}
              >
                {remove.isPending ? 'Deleting…' : 'Delete permanently'}
              </Button>
            </div>
          </div>
        )}
      </Modal>
    </>
  );
}
