import { useCallback, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { ChevronLeft, ChevronRight, X } from 'lucide-react';
import { assetUrl } from '../lib/api';

export interface LightboxPhoto {
  id: string;
  url: string;
  caption: string | null;
}

/**
 * Full-size viewer for the public photo gallery.
 *
 * Hand-rolled rather than pulling in a lightbox library: this is a dimmed
 * overlay, an image, and three key bindings — the same reasoning as the
 * hand-drawn tide graph and the solunar maths.
 *
 * `index` is the photo currently shown; `null` means closed. Navigation wraps,
 * so arrowing past either end lands back on the other, and the prev/next
 * controls are hidden entirely for a single photo where they'd be no-ops.
 */
export function Lightbox({
  photos,
  index,
  onClose,
  onNavigate,
}: {
  photos: LightboxPhoto[];
  index: number | null;
  onClose: () => void;
  onNavigate: (next: number) => void;
}) {
  const open = index !== null && index >= 0 && index < photos.length;
  const many = photos.length > 1;

  const step = useCallback(
    (delta: number) => {
      if (index === null || photos.length === 0) return;
      onNavigate((index + delta + photos.length) % photos.length);
    },
    [index, photos.length, onNavigate],
  );

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
      if (e.key === 'ArrowLeft') step(-1);
      if (e.key === 'ArrowRight') step(1);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose, step]);

  // The page behind must not scroll while the overlay covers it.
  useEffect(() => {
    if (!open) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = previous;
    };
  }, [open]);

  if (!open) return null;
  const photo = photos[index];

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 text-base font-normal normal-case tracking-normal">
      {/* Clicking the backdrop closes; the image itself sits above it. */}
      <div className="absolute inset-0 bg-charcoal/90" onClick={onClose} aria-hidden />

      <button
        onClick={onClose}
        aria-label="Close"
        className="absolute right-4 top-4 z-10 rounded-full bg-charcoal/60 p-2 text-cream transition hover:bg-charcoal/80"
      >
        <X size={20} />
      </button>

      {many && (
        <>
          <button
            onClick={() => step(-1)}
            aria-label="Previous photo"
            className="absolute left-2 z-10 rounded-full bg-charcoal/60 p-2 text-cream transition hover:bg-charcoal/80 sm:left-4"
          >
            <ChevronLeft size={24} />
          </button>
          <button
            onClick={() => step(1)}
            aria-label="Next photo"
            className="absolute right-2 z-10 rounded-full bg-charcoal/60 p-2 text-cream transition hover:bg-charcoal/80 sm:right-4"
          >
            <ChevronRight size={24} />
          </button>
        </>
      )}

      <figure
        role="dialog"
        aria-modal="true"
        aria-label={photo.caption ?? 'Camp photo'}
        className="relative flex max-h-full max-w-5xl flex-col items-center gap-3"
      >
        <img
          src={assetUrl(photo.url) ?? undefined}
          alt={photo.caption ?? ''}
          className="max-h-[80vh] w-auto max-w-full rounded-lg object-contain shadow-2xl"
        />
        <figcaption className="text-center text-sm text-cream/80">
          {photo.caption}
          {many && (
            <span className="ml-2 text-cream/50">
              {index + 1} / {photos.length}
            </span>
          )}
        </figcaption>
      </figure>
    </div>,
    document.body,
  );
}
