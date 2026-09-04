import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { format } from 'date-fns';
import { toast } from 'sonner';
import { Inbox as InboxIcon, Mail, MailOpen, Send, Trash2 } from 'lucide-react';
import {
  Button,
  Card,
  EmptyState,
  Field,
  Input,
  PageHeader,
  Spinner,
  Textarea,
  cx,
} from '../components/ui';
import { api, ApiError } from '../lib/api';
import { useAuth } from '../lib/auth';
import { useMessages, useUsers } from '../lib/queries';
import type { Message } from '../lib/types';

function who(m: Message): string {
  if (!m.sender_id) return 'Dulac My Camp';
  return m.sender_name || m.sender_email || 'Unknown sender';
}

/**
 * The inbox, shared by `/inbox` and the admin Messages tab.
 *
 * `all` is an admin-only view over every message in the system; the compose
 * box switches to a recipient picker in that mode.
 */
export function InboxView({ all = false }: { all?: boolean }) {
  const { user, isAdmin } = useAuth();
  const queryClient = useQueryClient();
  const { data, isLoading } = useMessages(all && isAdmin);
  const { data: users } = useUsers(isAdmin);

  const [recipient, setRecipient] = useState('');
  const [subject, setSubject] = useState('');
  const [body, setBody] = useState('');

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ['messages'] });

  const send = useMutation({
    mutationFn: () =>
      api<Message>('/messages', {
        method: 'POST',
        body: {
          recipient_id: isAdmin ? recipient || null : null,
          subject: subject.trim() || null,
          body,
        },
      }),
    onSuccess: () => {
      toast.success('Message sent.');
      setSubject('');
      setBody('');
      invalidate();
    },
    onError: (err) =>
      toast.error(err instanceof ApiError ? err.message : 'Could not send that message.'),
  });

  const markRead = useMutation({
    mutationFn: (id: string) => api(`/messages/${id}/read`, { method: 'PUT' }),
    onSuccess: invalidate,
  });

  const remove = useMutation({
    mutationFn: (id: string) => api(`/messages/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      toast.success('Message deleted.');
      invalidate();
    },
    onError: (err) =>
      toast.error(err instanceof ApiError ? err.message : 'Could not delete that message.'),
  });

  const messages = data ?? [];

  return (
    <div className="grid gap-6 lg:grid-cols-[1fr_320px]">
      <div className="space-y-3">
        {isLoading ? (
          <div className="flex justify-center py-20">
            <Spinner />
          </div>
        ) : messages.length === 0 ? (
          <EmptyState
            icon={<InboxIcon size={26} />}
            title="Nothing here yet"
            hint="Booking updates and messages from the camp land here."
          />
        ) : (
          messages.map((m) => {
            const mine = m.recipient_id === user?.id;
            const unread = mine && !m.is_read;
            return (
              <Card
                key={m.id}
                className={cx(
                  'transition',
                  unread && 'border-forest-400 bg-forest-50/60',
                )}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="flex items-center gap-2 text-sm font-bold text-charcoal">
                      {unread ? (
                        <Mail size={15} className="shrink-0 text-forest-600" />
                      ) : (
                        <MailOpen size={15} className="shrink-0 text-muted" />
                      )}
                      {m.subject || '(no subject)'}
                    </p>
                    <p className="mt-0.5 text-xs text-muted">
                      {mine ? `From ${who(m)}` : `To ${m.recipient_name || m.recipient_email}`} ·{' '}
                      {format(new Date(m.created_at), 'MMM d, yyyy · h:mm a')}
                    </p>
                  </div>
                  <div className="flex shrink-0 gap-1">
                    {unread && (
                      <button
                        onClick={() => markRead.mutate(m.id)}
                        title="Mark as read"
                        className="rounded p-1.5 text-muted hover:bg-cream-dark hover:text-charcoal"
                      >
                        <MailOpen size={15} />
                      </button>
                    )}
                    <button
                      onClick={() => remove.mutate(m.id)}
                      title="Delete"
                      className="rounded p-1.5 text-muted hover:bg-cream-dark hover:text-clay"
                    >
                      <Trash2 size={15} />
                    </button>
                  </div>
                </div>
                <p className="mt-2.5 whitespace-pre-wrap text-sm text-charcoal">{m.body}</p>
              </Card>
            );
          })
        )}
      </div>

      <Card className="h-fit">
        <h3 className="mb-3 font-display text-base font-bold text-charcoal">
          {isAdmin ? 'Send a message' : 'Message the camp'}
        </h3>

        <form
          className="space-y-3"
          onSubmit={(e) => {
            e.preventDefault();
            send.mutate();
          }}
        >
          {isAdmin ? (
            <Field label="To">
              <select
                required
                value={recipient}
                onChange={(e) => setRecipient(e.target.value)}
                className="w-full rounded-lg border border-sand bg-white px-3 py-2.5 text-sm"
              >
                <option value="">Choose someone…</option>
                {users?.map((u) => (
                  <option key={u.id} value={u.id}>
                    {u.full_name || u.email}
                  </option>
                ))}
              </select>
            </Field>
          ) : (
            <p className="text-sm text-muted">Goes straight to the camp admin.</p>
          )}

          <Field label="Subject">
            <Input value={subject} onChange={(e) => setSubject(e.target.value)} placeholder="Optional" />
          </Field>

          <Field label="Message">
            <Textarea
              required
              rows={5}
              value={body}
              onChange={(e) => setBody(e.target.value)}
              placeholder="What's on your mind?"
            />
          </Field>

          <Button type="submit" className="w-full" disabled={send.isPending || !body.trim()}>
            <Send size={15} /> {send.isPending ? 'Sending…' : 'Send'}
          </Button>
        </form>
      </Card>
    </div>
  );
}

export default function Inbox() {
  return (
    <div className="mx-auto max-w-5xl px-4 py-10">
      <PageHeader title="Inbox" subtitle="Booking updates and messages with the camp." />
      <InboxView />
    </div>
  );
}
