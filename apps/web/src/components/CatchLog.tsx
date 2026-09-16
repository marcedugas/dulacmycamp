import { useRef, useState } from 'react';
import { toast } from 'sonner';
import { Fish, ImagePlus, Plus, Trash2, X } from 'lucide-react';
import { Button, CountInput, Input, OptionalNumberInput, Spinner, cx } from './ui';
import { Lightbox } from './Lightbox';
import { api, ApiError, assetUrl } from '../lib/api';
import { useFishSpecies } from '../lib/queries';
import type { JournalCatch, JournalCatchInput, JournalPhoto } from '../lib/types';

/** A blank row, so "add a catch" always starts from the same place. */
export const emptyCatch = (): JournalCatchInput => ({
  id: null,
  species_id: null,
  length_inches: null,
  weight_lbs: null,
  quantity: 1,
  notes: null,
});

/**
 * Turns saved catches back into editable rows.
 *
 * The id comes back with them: photos hang off `journal_catches.id`, so a
 * row that resubmitted without one would be deleted and reinserted on save,
 * cascading its photos away.
 */
export const toCatchInputs = (catches: JournalCatch[]): JournalCatchInput[] =>
  catches.map((c) => ({
    id: c.id,
    species_id: c.species_id,
    length_inches: c.length_inches,
    weight_lbs: c.weight_lbs,
    quantity: c.quantity,
    notes: c.notes,
  }));

/** Every photo attached to any of these catches, for seeding {@link CatchLog}. */
export const catchPhotosOf = (catches: JournalCatch[]): JournalPhoto[] =>
  catches.flatMap((c) => c.photos);

const THUMB =
  'h-12 w-12 shrink-0 overflow-hidden rounded-md border border-sand bg-cream-dark object-cover';

/**
 * The photo attachment for one saved catch row: a compact "+" that opens the
 * file picker scoped to this catch, and thumbnails of what's already on it.
 *
 * Uploads go straight to the server, same as the entry's own photos — the
 * catch row already exists by the time this renders, so there is nothing to
 * stage and a half-finished upload can't be lost by navigating away.
 */
function CatchRowPhotos({
  entryId,
  catchId,
  photos,
  onChanged,
}: {
  entryId: string;
  catchId: string;
  photos: JournalPhoto[];
  onChanged: () => void;
}) {
  const fileInput = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [lightbox, setLightbox] = useState<number | null>(null);

  const upload = async (files: FileList) => {
    setBusy(true);
    try {
      // Sequential, not parallel: sort_order is assigned server-side from the
      // current max, so overlapping uploads would race for the same slot.
      for (const file of Array.from(files)) {
        const fd = new FormData();
        fd.append('file', file);
        fd.append('journal_catch_id', catchId);
        await api<JournalPhoto>(`/journal/${entryId}/photos`, { method: 'POST', body: fd });
      }
      onChanged();
    } catch (err) {
      toast.error(err instanceof ApiError ? err.message : 'Could not upload that photo.');
    } finally {
      setBusy(false);
    }
  };

  const remove = async (photoId: string) => {
    setBusy(true);
    try {
      await api(`/journal/photos/${photoId}`, { method: 'DELETE' });
      onChanged();
    } catch (err) {
      toast.error(err instanceof ApiError ? err.message : 'Could not remove that photo.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <input
        ref={fileInput}
        type="file"
        multiple
        accept="image/jpeg,image/png,image/webp"
        className="hidden"
        onChange={(e) => {
          if (e.target.files?.length) void upload(e.target.files);
          e.target.value = '';
        }}
      />
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <button
          type="button"
          disabled={busy}
          onClick={() => fileInput.current?.click()}
          title="Attach a photo to this catch"
          aria-label="Attach a photo to this catch"
          className="flex h-12 w-12 shrink-0 items-center justify-center rounded-md border border-dashed border-sand bg-white text-muted transition hover:border-forest-400 hover:text-forest-700 disabled:opacity-50"
        >
          {busy ? <Spinner className="h-4 w-4 border" /> : <ImagePlus size={16} />}
        </button>

        {photos.map((p, i) => (
          <div key={p.id} className="relative">
            <button
              type="button"
              onClick={() => setLightbox(i)}
              title={p.caption ?? 'Open this photo'}
              aria-label="Open this photo"
              className="block"
            >
              <img src={assetUrl(p.url) ?? undefined} alt={p.caption ?? ''} className={THUMB} />
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => remove(p.id)}
              title="Remove this photo"
              aria-label="Remove this photo"
              className="absolute -right-1.5 -top-1.5 rounded-full border border-sand bg-white p-0.5 text-muted shadow-sm transition hover:text-clay disabled:opacity-50"
            >
              <X size={12} />
            </button>
          </div>
        ))}
      </div>

      {/* The same viewer the camp gallery uses — one lightbox in this app. */}
      <Lightbox
        photos={photos}
        index={lightbox}
        onClose={() => setLightbox(null)}
        onNavigate={setLightbox}
      />
    </>
  );
}

/**
 * The catch log: repeatable rows of species / length / weight / quantity,
 * each able to carry its own photo.
 *
 * Species come from the admin-managed list, so a dropdown rather than free
 * text — but every measurement is optional, because plenty of real trips are
 * "we caught a mess of trout" with no tape measure involved.
 *
 * Photos need somewhere to hang, so the controls appear only for rows that
 * have actually been saved (`entryId` set, and the row carrying an id). On
 * the "share your story" form that is nothing yet, which is why that form
 * points people back here once the story is posted.
 */
export function CatchLog({
  catches,
  onChange,
  className,
  entryId,
  photos = [],
  onPhotosChanged,
}: {
  catches: JournalCatchInput[];
  onChange: (next: JournalCatchInput[]) => void;
  className?: string;
  /** The saved entry these rows belong to. Absent on an unsaved entry, which
   *  is what hides the photo controls. */
  entryId?: string;
  /** Every catch-tagged photo of that entry; each row picks out its own. */
  photos?: JournalPhoto[];
  onPhotosChanged?: () => void;
}) {
  const { data: species } = useFishSpecies();
  const options = species ?? [];

  const patch = (index: number, changes: Partial<JournalCatchInput>) =>
    onChange(catches.map((c, i) => (i === index ? { ...c, ...changes } : c)));

  const removeRow = (index: number) => onChange(catches.filter((_, i) => i !== index));

  return (
    <div className={cx('rounded-xl border border-sand bg-cream-dark/50 p-4', className)}>
      <div className="mb-1 flex items-center gap-2">
        <Fish size={16} className="text-forest-600" />
        <h3 className="text-sm font-bold text-charcoal">Catch log</h3>
      </div>
      <p className="mb-3 text-xs text-muted">
        Optional. Add a row per species — measurements can be left blank.
        {entryId && ' Each catch can carry its own photo.'}
      </p>

      {catches.length > 0 && (
        <ul className="mb-3 space-y-2">
          {catches.map((c, i) => (
            <li key={c.id ?? `new-${i}`} className="rounded-lg border border-sand bg-white p-3">
              <div className="grid gap-2 sm:grid-cols-[minmax(0,2fr)_repeat(3,minmax(0,1fr))_auto]">
                <select
                  aria-label="Species"
                  className="w-full rounded-lg border border-sand bg-white px-3 py-2.5 text-sm text-charcoal focus:border-forest-500 focus:outline-none"
                  value={c.species_id ?? ''}
                  onChange={(e) => patch(i, { species_id: e.target.value || null })}
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
                  onChange={(length_inches) => patch(i, { length_inches })}
                />
                <OptionalNumberInput
                  aria-label="Weight in pounds"
                  placeholder="Weight lb"
                  value={c.weight_lbs}
                  onChange={(weight_lbs) => patch(i, { weight_lbs })}
                />
                <CountInput
                  min={1}
                  step="1"
                  aria-label="Quantity"
                  placeholder="Qty"
                  value={c.quantity}
                  onChange={(quantity) => patch(i, { quantity })}
                />
                <button
                  type="button"
                  onClick={() => removeRow(i)}
                  title="Remove this catch"
                  aria-label="Remove this catch"
                  className="shrink-0 rounded p-2 text-muted hover:bg-cream-dark hover:text-clay"
                >
                  <Trash2 size={16} />
                </button>
              </div>
              <Input
                className="mt-2 text-xs"
                placeholder="Notes (where, what bait, who caught it…)"
                value={c.notes ?? ''}
                onChange={(e) => patch(i, { notes: e.target.value || null })}
              />
              {entryId && c.id && (
                <CatchRowPhotos
                  entryId={entryId}
                  catchId={c.id}
                  photos={photos.filter((p) => p.journal_catch_id === c.id)}
                  onChanged={() => onPhotosChanged?.()}
                />
              )}
              {entryId && !c.id && (
                <p className="mt-2 text-xs text-muted">
                  Save your changes to attach a photo to this catch.
                </p>
              )}
            </li>
          ))}
        </ul>
      )}

      <Button type="button" size="sm" variant="ghost" onClick={() => onChange([...catches, emptyCatch()])}>
        <Plus size={14} /> Add a catch
      </Button>
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
      c.quantity > 1 ? `${c.quantity}×` : null,
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
