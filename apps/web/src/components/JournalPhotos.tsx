import { useEffect, useRef, type LabelHTMLAttributes, type ReactNode } from 'react';
import { ImagePlus, Trash2, X } from 'lucide-react';
import { cx } from './ui';
import { assetUrl } from '../lib/api';
import {
  MAX_PHOTO_MB,
  MAX_PHOTOS,
  PHOTO_TYPES,
  draftPhotoItems,
  type DraftPhotoItem,
  type StagedPhoto,
} from '../lib/journalDraft';
import type { JournalPhoto } from '../lib/types';

/**
 * A local preview of a picked-but-not-uploaded file.
 *
 * The object URL is created and revoked by the same effect, so it lives
 * exactly as long as the thumbnail showing it — including under StrictMode's
 * mount/unmount/mount, where revoking in a separate cleanup would leave the
 * remounted thumbnail pointing at a dead URL. It's set on the element
 * directly: the URL is an external resource, not render state.
 */
function StagedImg({ file, className }: { file: File; className: string }) {
  const img = useRef<HTMLImageElement>(null);
  useEffect(() => {
    const url = URL.createObjectURL(file);
    if (img.current) img.current.src = url;
    return () => URL.revokeObjectURL(url);
  }, [file]);
  return <img ref={img} alt="" className={className} />;
}

const itemKey = (item: DraftPhotoItem) => (item.kind === 'saved' ? item.photo.id : item.staged.key);

/** One photo, saved or staged, as an <img> with the given classes. */
export function DraftPhotoImg({ item, className }: { item: DraftPhotoItem; className: string }) {
  return item.kind === 'saved' ? (
    <img src={assetUrl(item.photo.url) ?? undefined} alt={item.photo.caption ?? ''} className={className} />
  ) : (
    <StagedImg file={item.staged.file} className={className} />
  );
}

/**
 * A file picker that looks like whatever `children` is. A <label> around the
 * input opens it natively, and the input is visually hidden rather than
 * `display: none`, so it stays in the tab order — Enter or Space on it opens
 * the picker too.
 *
 * `onPick` gets every chosen file; screening them (type, size, the photo
 * cap) is the caller's job, so the gallery and each catch row share one set
 * of rules.
 */
export function PhotoPicker({
  onPick,
  disabled,
  className,
  children,
  ...rest
}: {
  onPick: (files: File[]) => void;
  disabled?: boolean;
  className?: string;
  children: ReactNode;
} & Omit<LabelHTMLAttributes<HTMLLabelElement>, 'children' | 'className'>) {
  return (
    <label className={cx('focus-within:ring-2 focus-within:ring-forest-400', className)} {...rest}>
      <input
        type="file"
        multiple
        accept={PHOTO_TYPES.join(',')}
        disabled={disabled}
        className="sr-only"
        onChange={(e) => {
          if (e.target.files?.length) onPick(Array.from(e.target.files));
          e.target.value = '';
        }}
      />
      {children}
    </label>
  );
}

/**
 * The story's general photos while composing: ones already posted, plus
 * newly picked ones shown from the device. Nothing uploads from here — the
 * form's Post / Save Changes does it all at once.
 */
export function DraftPhotoGallery({
  saved,
  staged,
  onPick,
  onRemoveSaved,
  onRemoveStaged,
  atCap,
}: {
  saved: JournalPhoto[];
  staged: StagedPhoto[];
  onPick: (files: File[]) => void;
  onRemoveSaved: (photoId: string) => void;
  onRemoveStaged: (key: string) => void;
  atCap: boolean;
}) {
  const items = draftPhotoItems(saved, staged);
  return (
    <div className="rounded-xl border border-sand bg-cream-dark/50 p-4">
      <div className="mb-1 flex items-center gap-2">
        <ImagePlus size={16} className="text-forest-600" />
        <h3 className="text-sm font-bold text-charcoal">Photos</h3>
      </div>
      <p className="mb-3 text-xs text-muted">
        Optional. JPG, PNG, or WEBP — up to {MAX_PHOTO_MB}MB each, {MAX_PHOTOS} per story. For a photo
        of a particular fish, use Attach Photo on that catch instead.
      </p>

      {items.length > 0 && (
        <div className="mb-3 grid grid-cols-2 gap-3 sm:grid-cols-3">
          {items.map((item) => (
            <div key={itemKey(item)} className="overflow-hidden rounded-lg border border-sand bg-white">
              <div className="aspect-4/3 bg-cream-dark">
                <DraftPhotoImg item={item} className="h-full w-full object-cover" />
              </div>
              <button
                type="button"
                onClick={() =>
                  item.kind === 'saved' ? onRemoveSaved(item.photo.id) : onRemoveStaged(item.staged.key)
                }
                className="flex w-full items-center justify-center gap-1.5 px-2 py-1.5 text-xs font-semibold text-muted hover:bg-cream-dark hover:text-clay"
              >
                <Trash2 size={13} /> Remove
              </button>
            </div>
          ))}
        </div>
      )}

      <PhotoPicker
        onPick={onPick}
        disabled={atCap}
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          e.preventDefault();
          if (!atCap && e.dataTransfer.files?.length) onPick(Array.from(e.dataTransfer.files));
        }}
        className={cx(
          'flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-sand bg-white/60 px-6 py-5 text-center text-muted transition',
          atCap ? 'cursor-not-allowed opacity-60' : 'cursor-pointer hover:border-forest-400 hover:text-forest-700',
        )}
      >
        <ImagePlus size={20} />
        <span className="text-sm font-semibold">
          {atCap ? `This story has the most photos it can hold (${MAX_PHOTOS}).` : 'Attach Photo'}
        </span>
        {!atCap && <span className="text-xs">Tap to choose, or drag photos here</span>}
      </PhotoPicker>
    </div>
  );
}

/** A small square thumbnail with a remove ✕ — the catch row's photo size. */
export function DraftThumb({ item, onRemove }: { item: DraftPhotoItem; onRemove: () => void }) {
  return (
    <div className="relative">
      <DraftPhotoImg
        item={item}
        className="h-14 w-14 shrink-0 overflow-hidden rounded-md border border-sand bg-cream-dark object-cover"
      />
      <button
        type="button"
        onClick={onRemove}
        title="Remove this photo"
        aria-label="Remove this photo"
        className="absolute -right-1.5 -top-1.5 rounded-full border border-sand bg-white p-0.5 text-muted shadow-sm transition hover:text-clay"
      >
        <X size={12} />
      </button>
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
