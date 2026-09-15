import { useState } from 'react';
import { Link } from 'react-router-dom';
import { format } from 'date-fns';
import { BookOpen, ChevronLeft, ChevronRight, Pencil } from 'lucide-react';
import {
  Button,
  Card,
  EmptyState,
  JournalVisibilityBadge,
  PageHeader,
  Spinner,
} from '../components/ui';
import { CatchSummary } from '../components/CatchLog';
import { PhotoStrip } from '../components/JournalPhotos';
import { useJournalFeed } from '../lib/queries';
import { formatRange } from '../lib/dates';

/**
 * The camp journal. Deliberately not a review system — no stars, no rating
 * UI anywhere on this page, just stories, catches and photos.
 *
 * Login required (see the route guard in `App.tsx`): entries are scoped
 * "family" vs "any registered account", so there is no longer an anonymous
 * tier this page could serve. What each reader gets back is decided
 * server-side by their role — a guest sees public entries, family sees those
 * plus family-only ones, and an admin or the camp owner sees everything
 * including hidden entries, badged as such.
 */
export default function Journal() {
  const [page, setPage] = useState(1);
  const { data, isLoading } = useJournalFeed(page);

  const entries = data?.entries ?? [];
  const totalPages = data?.total_pages ?? 1;
  const moderator = data?.moderator ?? false;

  return (
    <div className="mx-auto max-w-3xl px-4 py-10">
      <PageHeader
        title="Camp journal"
        subtitle="Stories, fishing reports, and memories from Dulac My Camp."
      />

      {moderator && (
        <p className="mb-5 rounded-lg border border-wood-300 bg-wood-100 px-3 py-2 text-sm text-wood-800">
          You're seeing every entry, including family-only and hidden ones. Each is badged with who
          can actually read it.
        </p>
      )}

      {isLoading ? (
        <div className="flex justify-center py-20">
          <Spinner />
        </div>
      ) : entries.length === 0 ? (
        <EmptyState
          icon={<BookOpen size={26} />}
          title="No stories yet"
          hint="Be the first to share one after your stay!"
        />
      ) : (
        <>
          <div className="space-y-5">
            {entries.map((e) => (
              <Card key={e.id}>
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div className="min-w-0">
                    <h3 className="font-display text-xl font-bold text-charcoal">{e.title}</h3>
                    <p className="mt-0.5 text-xs font-semibold uppercase tracking-wide text-bayou-600">
                      Visited {formatRange(e.check_in, e.check_out)}
                    </p>
                  </div>
                  <div className="flex shrink-0 items-center gap-1.5">
                    {/* Own entries are badged for everyone: the author is the
                        one reader who sees theirs whatever state it's in, so
                        without this a hidden entry would look live to them. */}
                    {(e.is_mine || moderator) && (
                      <JournalVisibilityBadge
                        visibility={e.visibility}
                        archived={Boolean(e.archived_at)}
                      />
                    )}
                    {e.is_mine && (
                      <Link to={`/journal/new?entry_id=${e.id}`}>
                        <Button size="sm" variant="ghost">
                          <Pencil size={13} /> Edit
                        </Button>
                      </Link>
                    )}
                  </div>
                </div>

                <p className="mt-3 whitespace-pre-wrap text-sm leading-relaxed text-charcoal">
                  {e.body}
                </p>

                <CatchSummary catches={e.catches} />
                <PhotoStrip photos={e.photos} />

                <p className="mt-4 text-xs text-muted">
                  — {e.is_mine ? 'You' : e.guest_first_name}, posted{' '}
                  {format(new Date(e.created_at), 'MMM d, yyyy')}
                </p>
              </Card>
            ))}
          </div>

          {totalPages > 1 && (
            <div className="mt-8 flex items-center justify-center gap-3">
              <Button
                variant="ghost"
                size="sm"
                disabled={page <= 1}
                onClick={() => setPage((p) => Math.max(1, p - 1))}
              >
                <ChevronLeft size={15} /> Newer
              </Button>
              <span className="text-sm text-muted">
                Page {page} of {totalPages}
              </span>
              <Button
                variant="ghost"
                size="sm"
                disabled={page >= totalPages}
                onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
              >
                Older <ChevronRight size={15} />
              </Button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
