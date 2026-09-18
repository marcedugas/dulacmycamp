import { useState } from 'react';
import { Fish, ImagePlus, Plus, Trash2 } from 'lucide-react';
import { Button, Input, OptionalNumberInput, cx } from './ui';
import { Lightbox } from './Lightbox';
import { DraftThumb, PhotoPicker } from './JournalPhotos';
import { assetUrl } from '../lib/api';
import { useFishSpecies } from '../lib/queries';
import { MAX_CATCHES, draftPhotoItems, emptyCatch, type DraftCatch } from '../lib/journalDraft';
import type { JournalCatch } from '../lib/types';

const THUMB =
  'h-12 w-12 shrink-0 overflow-hidden rounded-md border border-sand bg-cream-dark object-cover';

/**
 * The catch log while composing: repeatable rows of species / length /
 * weight, each able to carry its own photos. All of it is local — adding a
 * row, removing one, attaching a photo — until the form is posted.
 *
 * Species come from the admin-managed list, so a dropdown rather than free
 * text — but every measurement is optional, because plenty of real trips are
 * "we caught a mess of trout" with no tape measure involved.
 *
 * Quantity is no longer asked for: one row per fish worth remembering reads
 * better than a count, and the column keeps whatever older entries stored.
 */
export function CatchLog({
  catches,
  onChange,
  onPickPhotos,
  photosAtCap,
  className,
}: {
  catches: DraftCatch[];
  onChange: (next: DraftCatch[]) => void;
  /** Screens and stages files onto the row with this key. */
  onPickPhotos: (catchKey: string, files: File[]) => void;
  photosAtCap: boolean;
  className?: string;
}) {
  const { data: species } = useFishSpecies();
  const options = species ?? [];
  const atCap = catches.length >= MAX_CATCHES;

  const patch = (key: string, changes: Partial<DraftCatch>) =>
    onChange(catches.map((c) => (c.key === key ? { ...c, ...changes } : c)));

  const removeRow = (key: string) => onChange(catches.filter((c) => c.key !== key));

  return (
    <div className={cx('rounded-xl border border-sand bg-cream-dark/50 p-4', className)}>
      <div className="mb-1 flex items-center gap-2">
        <Fish size={16} className="text-forest-600" />
        <h3 className="text-sm font-bold text-charcoal">Catch log</h3>
      </div>
      <p className="mb-3 text-xs text-muted">
        Optional. Add a row per fish — measurements can be left blank, and each catch can have its
        own photo.
      </p>

      {catches.length > 0 && (
        <ul className="mb-3 space-y-2">
          {catches.map((c) => {
            const photos = draftPhotoItems(c.photos, c.staged);
            return (
              <li key={c.key} className="rounded-lg border border-sand bg-white p-3">
                <div className="grid gap-2 sm:grid-cols-[minmax(0,2fr)_repeat(2,minmax(0,1fr))_auto]">
                  <select
                    aria-label="Species"
                    className="w-full rounded-lg border border-sand bg-white px-3 py-2.5 text-sm text-charcoal focus:border-forest-500 focus:outline-none"
                    value={c.species_id ?? ''}
                    onChange={(e) => patch(c.key, { species_id: e.target.value || null })}
                  >
                    <option value="">Species…</option>
                    {options.map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.name}
                      </option>
                    ))}
                  </select>
                  <OptionalNumberInput
                    aria-label="Length in inches"
                    placeholder='Length"'
                    value={c.length_inches}
                    onChange={(length_inches) => patch(c.key, { length_inches })}
                  />
                  <OptionalNumberInput
                    aria-label="Weight in pounds"
                    placeholder="Weight lb"
                    value={c.weight_lbs}
                    onChange={(weight_lbs) => patch(c.key, { weight_lbs })}
                  />
                  <button
                    type="button"
                    onClick={() => removeRow(c.key)}
                    title="Remove this catch"
                    aria-label="Remove this catch"
                    className="flex shrink-0 items-center justify-center gap-1.5 rounded p-2 text-sm text-muted hover:bg-cream-dark hover:text-clay"
                  >
                    <Trash2 size={16} /> <span className="sm:hidden">Remove</span>
                  </button>
                </div>
                <Input
                  className="mt-2 text-xs"
                  placeholder="Notes (where, what bait, who caught it…)"
                  value={c.notes ?? ''}
                  onChange={(e) => patch(c.key, { notes: e.target.value || null })}
                />
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  {photos.map((item) => (
                    <DraftThumb
                      key={item.kind === 'saved' ? item.photo.id : item.staged.key}
                      item={item}
                      onRemove={() =>
                        item.kind === 'saved'
                          ? patch(c.key, { photos: c.photos.filter((p) => p.id !== item.photo.id) })
                          : patch(c.key, { staged: c.staged.filter((s) => s.key !== item.staged.key) })
                      }
                    />
                  ))}
                  <PhotoPicker
                    onPick={(files) => onPickPhotos(c.key, files)}
                    disabled={photosAtCap}
                    title={photosAtCap ? 'This story has the most photos it can hold.' : undefined}
                    className={cx(
                      'inline-flex items-center gap-1.5 rounded-md border border-dashed border-sand bg-white px-3 py-2 text-xs font-semibold text-muted transition',
                      photosAtCap
                        ? 'cursor-not-allowed opacity-50'
                        : 'cursor-pointer hover:border-forest-400 hover:text-forest-700',
                    )}
                  >
                    <ImagePlus size={14} /> Attach Photo
                  </PhotoPicker>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      <Button
        type="button"
        size="sm"
        variant="ghost"
        disabled={atCap}
        onClick={() => onChange([...catches, emptyCatch()])}
      >
        <Plus size={14} /> {catches.length === 0 ? 'Add a Catch' : 'Add Another Catch'}
      </Button>
      {atCap && (
        <p className="mt-2 text-xs text-muted">A story can log up to {MAX_CATCHES} catches.</p>
      )}
    </div>
  );
}

/**
 * Read-only summary of a saved catch log, for the feed.
 *
 * A catch that carries photos shows them as thumbnails under its own pill,
 * rather than in the entry's gallery strip — this *is* where they live, so
 * the photo reads as belonging to that fish.
 */
export function CatchSummary({ catches }: { catches: JournalCatch[] }) {
  // One index across the whole log, so at most one lightbox is ever open —
  // but the photos it navigates are only the clicked catch's own.
  const [open, setOpen] = useState<{ catchId: string; index: number } | null>(null);

  if (catches.length === 0) return null;

  const describe = (c: JournalCatch) => {
    const bits = [
      c.species_name ?? 'Unspecified',
      c.length_inches != null ? `${c.length_inches}"` : null,
      c.weight_lbs != null ? `${c.weight_lbs} lb` : null,
    ].filter(Boolean);
    return bits.join(' ');
  };

  const openCatch = open ? catches.find((c) => c.id === open.catchId) : undefined;

  return (
    <>
      <ul className="mt-3 flex flex-wrap items-start gap-1.5">
        {catches.map((c) => (
          <li key={c.id} className="inline-flex flex-col items-start gap-1">
            <span
              title={c.notes ?? undefined}
              className="inline-flex items-center gap-1.5 rounded-full border border-bayou-300 bg-bayou-50 px-2.5 py-0.5 text-xs font-semibold text-bayou-800"
            >
              <Fish size={12} /> {describe(c)}
            </span>
            {c.photos.length > 0 && (
              <div className="flex flex-wrap gap-1">
                {c.photos.map((p, i) => (
                  <button
                    key={p.id}
                    type="button"
                    onClick={() => setOpen({ catchId: c.id, index: i })}
                    title={p.caption ?? 'Open this photo'}
                    aria-label="Open this photo"
                    className="block"
                  >
                    <img
                      src={assetUrl(p.url) ?? undefined}
                      alt={p.caption ?? ''}
                      loading="lazy"
                      className={cx(THUMB, 'transition hover:brightness-110')}
                    />
                  </button>
                ))}
              </div>
            )}
          </li>
        ))}
      </ul>

      <Lightbox
        photos={openCatch?.photos ?? []}
        index={open?.index ?? null}
        onClose={() => setOpen(null)}
        onNavigate={(index) => setOpen((o) => (o ? { ...o, index } : o))}
      />
    </>
  );
}
