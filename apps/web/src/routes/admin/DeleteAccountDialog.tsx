import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Button, Field, Input, Modal, Spinner } from '../../components/ui';
import { api, ApiError } from '../../lib/api';
import type { DeletePreview, UserWithStats } from '../../lib/types';

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** "3 bookings (1 approved), 2 messages, 0 stories, 5 photos" */
function describe(p: DeletePreview): string {
  const total =
    p.bookings_pending + p.bookings_approved + p.bookings_denied + p.bookings_cancelled;
  return [
    `${plural(total, 'booking')} (${p.bookings_approved} approved)`,
    plural(p.messages, 'message'),
    plural(p.stories, 'story', 'stories'),
    plural(p.photos, 'photo'),
  ].join(', ');
}

/**
 * Permanent account deletion. Loads a counts-only preview, says plainly what
 * goes, and keeps the button off until the account's email has been typed.
 * The server checks the email again and has the last word on what is refused.
 */
export default function DeleteAccountDialog({
  user,
  onClose,
}: {
  user: UserWithStats | null;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const [typed, setTyped] = useState('');
  const [error, setError] = useState<string | null>(null);

  const preview = useQuery({
    queryKey: ['delete-preview', user?.id],
    queryFn: () => api<DeletePreview>(`/users/${user!.id}/delete-preview`),
    enabled: user !== null,
    gcTime: 0,
  });

  const close = () => {
    setTyped('');
    setError(null);
    onClose();
  };

  const remove = useMutation({
    mutationFn: () =>
      api<{ files_failed: number }>(`/users/${user!.id}`, {
        method: 'DELETE',
        body: { confirm_email: typed },
      }),
    onSuccess: (res) => {
      toast.success(
        res.files_failed > 0
          ? `Account deleted. ${plural(res.files_failed, 'file')} could not be removed and need a manual sweep.`
          : 'Account permanently deleted.',
      );
      void queryClient.invalidateQueries({ queryKey: ['users'] });
      void queryClient.invalidateQueries({ queryKey: ['bookings'] });
      close();
    },
    onError: (err) => {
      setError(err instanceof ApiError ? err.message : 'Could not delete that account.');
      void queryClient.invalidateQueries({ queryKey: ['users'] });
    },
  });

  const p = preview.data;
  const matches =
    user !== null && typed.trim().toLowerCase() === user.email.toLowerCase() && typed.trim() !== '';

  return (
    <Modal open={user !== null} onClose={close} title="Delete this account?">
      <div className="space-y-4">
        {preview.isLoading && (
          <div className="flex justify-center py-4">
            <Spinner />
          </div>
        )}
        {preview.isError && (
          <p className="text-sm text-clay">Could not load what this would remove. Try again.</p>
        )}
        {p?.refusal && <p className="text-sm text-clay">{p.refusal}</p>}

        {p && !p.refusal && user && (
          <>
            <p className="text-sm text-charcoal">
              This permanently removes <strong>{user.full_name ?? user.email}</strong> and
              everything about them: <strong>{describe(p)}</strong>.
              {p.bookings_approved > 0 && ' Their approved stays will disappear from the calendar.'}
              {p.bookings_future > 0 &&
                ` ${plural(p.bookings_future, 'booking')} ${p.bookings_future === 1 ? 'is' : 'are'} still upcoming or in progress.`}
            </p>
            <p className="rounded-lg border border-clay/30 bg-clay/5 px-3 py-2 text-sm text-clay">
              This is permanent and cannot be undone. If they might be back, use{' '}
              <strong>Block</strong> instead — it keeps their record and can be reversed.
            </p>
            <Field label={`Type ${user.email} to confirm`}>
              <Input
                value={typed}
                onChange={(e) => setTyped(e.target.value)}
                autoComplete="off"
                autoCapitalize="none"
                spellCheck={false}
                placeholder={user.email}
              />
            </Field>
          </>
        )}

        {error && <p className="text-sm text-clay">{error}</p>}

        <div className="flex justify-end gap-2">
          <Button variant="ghost" type="button" onClick={close}>
            Cancel
          </Button>
          <Button
            variant="danger"
            type="button"
            disabled={!p || Boolean(p.refusal) || !matches || remove.isPending}
            onClick={() => remove.mutate()}
          >
            {remove.isPending ? 'Deleting…' : 'Delete permanently'}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
