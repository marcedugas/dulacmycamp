import { useRef, useState } from 'react';
import { toast } from 'sonner';
import { ImagePlus, Trash2 } from 'lucide-react';
import { Spinner, cx } from './ui';
import { api, ApiError, assetUrl } from '../lib/api';
import type { JournalPhoto } from '../lib/types';

/**
 * Photo attachments for one entry, uploaded one file at a time against the
 * same plumbing as the site gallery.
 *
 * Uploads go straight to the server rather than being held until save: an
 * entry always exists by the time this renders, so there is nothing to
 * stage, and a half-finished upload can't be lost by navigating away.
 */
export function JournalPhotos({
  entryId,
  photos,
  onChanged,
  editable,
}: {
  entryId: string;
  photos: JournalPhoto[];
  onChanged: () => void;
  editable: boolean;
}) {
  const fileInput = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);

  const upload = async (files: FileList) => {
    setBusy(true);
    try {
      // Sequential, not parallel: sort_order is assigned server-side from the
      // current max, so overlapping uploads would race for the same slot.
      for (const file of Array.from(files)) {
        const fd = new FormData();
        fd.append('file', file);
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
    <div className="rounded-xl border border-sand bg-cream-dark/50 p-4">
      <div className="mb-1 flex items-center gap-2">
        <ImagePlus size={16} className="text-forest-600" />
        <h3 className="text-sm font-bold text-charcoal">Photos</h3>
      </div>
      <p className="mb-3 text-xs text-muted">Optional. JPG, PNG, or WEBP — up to 8MB each.</p>

      {photos.length > 0 && (
        <div className="mb-3 grid grid-cols-2 gap-3 sm:grid-cols-3">
          {photos.map((p) => (
            <div key={p.id} className="overflow-hidden rounded-lg border border-sand bg-white">
              <div className="aspect-4/3 bg-cream-dark">
                <img
                  src={assetUrl(p.url) ?? undefined}
                  alt={p.caption ?? ''}
                  className="h-full w-full object-cover"
                />
              </div>
              {editable && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => remove(p.id)}
                  className="flex w-full items-center justify-center gap-1.5 px-2 py-1.5 text-xs font-semibold text-muted hover:bg-cream-dark hover:text-clay disabled:opacity-50"
                >
                  <Trash2 size={13} /> Remove
                </button>
              )}
            </div>
          ))}
        </div>
      )}

      {editable && (
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
          <div
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              e.preventDefault();
              if (e.dataTransfer.files?.length) void upload(e.dataTransfer.files);
            }}
            onClick={() => !busy && fileInput.current?.click()}
            className={cx(
              'flex cursor-pointer flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-sand bg-white/60 px-6 py-6 text-center text-muted transition',
              busy ? 'opacity-60' : 'hover:border-forest-400 hover:text-forest-700',
            )}
          >
            {busy ? <Spinner /> : <ImagePlus size={20} />}
            <p className="text-sm font-semibold">
              {busy ? 'Uploading…' : 'Click or drag photos here to add them'}
            </p>
          </div>
        </>
      )}
    </div>
  );
}

/** Read-only thumbnails for the feed. */
export function PhotoStrip({ photos }: { photos: JournalPhoto[] }) {
  if (photos.length === 0) return null;
  return (
    <div className="mt-4 grid grid-cols-2 gap-2 sm:grid-cols-3">
      {photos.map((p) => (
        <a
          key={p.id}
          href={assetUrl(p.url) ?? undefined}
          target="_blank"
          rel="noreferrer noopener"
          className="block overflow-hidden rounded-lg border border-sand bg-cream-dark"
        >
          <div className="aspect-4/3">
            <img
              src={assetUrl(p.url) ?? undefined}
              alt={p.caption ?? ''}
              loading="lazy"
              className="h-full w-full object-cover transition hover:scale-105"
            />
          </div>
        </a>
      ))}
    </div>
  );
}
