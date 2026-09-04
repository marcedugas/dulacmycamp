import { useState } from 'react';
import { format } from 'date-fns';
import { BookOpen, ChevronLeft, ChevronRight } from 'lucide-react';
import { Button, Card, EmptyState, PageHeader, Spinner } from '../components/ui';
import { useJournalPublic } from '../lib/queries';
import { formatRange } from '../lib/dates';

/**
 * The public camp journal. Deliberately not a review system — no stars, no
 * rating UI anywhere on this page, just stories.
 */
export default function Journal() {
  const [page, setPage] = useState(1);
  const { data, isLoading } = useJournalPublic(page);

  const entries = data?.entries ?? [];
  const totalPages = data?.total_pages ?? 1;

  return (
    <div className="mx-auto max-w-3xl px-4 py-10">
      <PageHeader title="Camp journal" subtitle="Stories, fishing reports, and memories from Dulac My Camp." />

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
                <h3 className="font-display text-xl font-bold text-charcoal">{e.title}</h3>
                <p className="mt-0.5 text-xs font-semibold uppercase tracking-wide text-bayou-600">
                  Visited {formatRange(e.check_in, e.check_out)}
                </p>
                <p className="mt-3 whitespace-pre-wrap text-sm leading-relaxed text-charcoal">{e.body}</p>
                <p className="mt-4 text-xs text-muted">
                  — {e.guest_first_name}
                  {e.approved_at && `, posted ${format(new Date(e.approved_at), 'MMM d, yyyy')}`}
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
