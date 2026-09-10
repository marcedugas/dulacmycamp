import { useMemo, useState } from 'react';
import { ExternalLink, Image as ImageIcon, Images, MapPin, Navigation } from 'lucide-react';
import { Button, Card, EmptyState, Spinner, cx } from '../components/ui';
import { Lightbox } from './Lightbox';
import { assetUrl } from '../lib/api';
import { useGuestPhotosLink } from '../lib/queries';
import { amenityIcon } from '../lib/icons';
import type { AboutSection, SiteContent } from '../lib/types';
import { ABOUT_SECTIONS, ABOUT_SECTION_LABELS } from '../lib/types';

type Photo = SiteContent['gallery'][number];
type Amenity = SiteContent['amenities'][number];

/**
 * Where the camp is, and a link to drive there — or nothing at all when no
 * address is set.
 *
 * Google Maps takes the destination as a plain query parameter and accepts a
 * street address and a bare "lat,lng" pair interchangeably, so whatever the
 * admin typed goes through url-encoded and unparsed. No key, no SDK.
 */
function Directions({ address }: { address: string | null }) {
  const trimmed = address?.trim();
  if (!trimmed) return null;

  const href = `https://www.google.com/maps/dir/?api=1&destination=${encodeURIComponent(trimmed)}`;
  return (
    <Card className="flex flex-wrap items-center justify-between gap-3">
      <p className="flex items-center gap-2 text-sm font-semibold text-charcoal">
        <MapPin size={16} className="shrink-0 text-forest-600" />
        {trimmed}
      </p>
      <a href={href} target="_blank" rel="noreferrer noopener">
        <Button size="sm" variant="secondary">
          <Navigation size={14} /> Get Directions
        </Button>
      </a>
    </Card>
  );
}

/**
 * The shared Google Photos album — everyone's own trip pictures, as distinct
 * from the camp's public gallery above it.
 *
 * Who may see it is the API's decision, not this component's: eligibility is
 * `role == 'user'` OR an ever-approved booking, and anyone else gets a 403,
 * which the query surfaces as an error and this renders as nothing. Logged
 * out, the query never fires at all. Same pattern as My Stay's copy of this.
 */
function GroupPhotosLink() {
  const { data } = useGuestPhotosLink();
  if (!data?.url) return null;

  return (
    <Card className="mt-6 flex flex-wrap items-center justify-between gap-3">
      <div className="min-w-0">
        <p className="flex items-center gap-2 font-semibold text-charcoal">
          <Images size={16} className="shrink-0 text-forest-600" /> Everyone's photos
        </p>
        <p className="mt-1 text-sm text-muted">
          The shared album from everyone who stays here — add yours to it.
        </p>
      </div>
      <a href={data.url} target="_blank" rel="noreferrer noopener">
        <Button size="sm" variant="secondary">
          Open the album <ExternalLink size={14} />
        </Button>
      </a>
    </Card>
  );
}

/** One section's photos, each opening the lightbox at its own position. */
function PhotoGrid({
  photos,
  onOpen,
  showEmpty,
  wide = false,
}: {
  photos: Photo[];
  onOpen: (index: number) => void;
  showEmpty: boolean;
  /** Full width of the tab rather than confined to the right column. */
  wide?: boolean;
}) {
  if (photos.length === 0) {
    if (!showEmpty) return null;
    return (
      <div className="flex aspect-4/3 max-w-xs flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-sand bg-cream-dark/60 text-muted">
        <ImageIcon size={22} />
        <span className="text-xs font-semibold uppercase tracking-wide">Photos coming soon</span>
      </div>
    );
  }

  return (
    <div className={cx('grid gap-3', wide ? 'grid-cols-2 sm:grid-cols-3 lg:grid-cols-4' : 'grid-cols-2')}>
      {photos.map((g, i) => (
        <button
          key={g.id}
          type="button"
          onClick={() => onOpen(i)}
          aria-label={g.caption ? `View ${g.caption}` : 'View photo'}
          className="group relative aspect-4/3 cursor-zoom-in overflow-hidden rounded-xl border border-sand bg-cream-dark/60"
        >
          <img
            src={assetUrl(g.url) ?? undefined}
            alt={g.caption ?? ''}
            className="h-full w-full object-cover transition group-hover:scale-105"
          />
          {g.caption && (
            <figcaption className="absolute inset-x-0 bottom-0 bg-charcoal/70 px-2 py-1 text-xs font-semibold text-cream">
              {g.caption}
            </figcaption>
          )}
        </button>
      ))}
    </div>
  );
}

/**
 * The three About stories as tabs, each with its own photos.
 *
 * Hand-rolled: the whole of a tab set is one piece of state and a conditional
 * render, which is not worth a dependency — same call as the tide graph and
 * the solunar maths.
 *
 * A tab appears only once it has something to show (its own text or its own
 * photos); the camp tab always does, since it carries the amenities and the
 * directions besides. That keeps a freshly-migrated site looking exactly as
 * it did rather than sprouting two empty headings, and it is the same rule
 * the stacked sections used before they became tabs. With only one tab
 * showing there is nothing to switch between, so the tab bar hides itself.
 */
export function AboutTabs({
  content,
  isLoading,
  capacityAdults,
}: {
  content: SiteContent | undefined;
  isLoading: boolean;
  capacityAdults: number;
}) {
  const [active, setActive] = useState<AboutSection>('camp');
  // Index into the *active* section's photos, so it must reset whenever the
  // tab changes — an index valid for one section can be out of range, or
  // simply the wrong photo, in another.
  const [lightbox, setLightbox] = useState<number | null>(null);

  const bodies: Record<AboutSection, string> = useMemo(
    () => ({
      camp: content?.about_camp_text?.trim() ?? '',
      dulac: content?.about_dulac_text?.trim() ?? '',
      last_island: content?.last_island_text?.trim() ?? '',
    }),
    [content],
  );

  const photosBySection = useMemo(() => {
    const empty: Record<AboutSection, Photo[]> = { camp: [], dulac: [], last_island: [] };
    for (const photo of content?.gallery ?? []) {
      // Ignore a section value the frontend doesn't know — a photo is never
      // worth a blank page.
      if (photo.about_section in empty) empty[photo.about_section].push(photo);
    }
    return empty;
  }, [content]);

  const amenities: Amenity[] = content?.amenities ?? [];
  const visible = ABOUT_SECTIONS.filter(
    (s) => s === 'camp' || bodies[s].length > 0 || photosBySection[s].length > 0,
  );

  const select = (section: AboutSection) => {
    setActive(section);
    setLightbox(null);
  };

  // Guard against a tab disappearing under the selection (an admin clearing a
  // section while someone is reading it).
  const current = visible.includes(active) ? active : 'camp';
  const photos = photosBySection[current];

  // On the camp tab the right column is the amenities alone — its gallery is
  // wider than half a row deserves and runs full width underneath instead.
  // The history tabs keep photos in the right column, where they read fine
  // against a short piece of prose.
  const galleryBelow = current === 'camp';
  const hasRightColumn = current === 'camp' || photos.length > 0;

  const campFallback = `Family and friends only. The camp sleeps ${capacityAdults} adults comfortably — more with kids on the bunks.`;
  const body = current === 'camp' ? bodies.camp || campFallback : bodies[current];

  return (
    <section id="about" className="mx-auto max-w-6xl px-4 py-14 sm:py-20">
      <h2 className="font-display text-3xl font-extrabold text-charcoal sm:text-4xl">About</h2>

      {visible.length > 1 && (
        <div role="tablist" aria-label="About sections" className="mt-5 flex flex-wrap gap-1 border-b border-sand">
          {visible.map((section) => (
            <button
              key={section}
              role="tab"
              id={`about-tab-${section}`}
              aria-selected={current === section}
              aria-controls={`about-panel-${section}`}
              onClick={() => select(section)}
              className={cx(
                '-mb-px whitespace-nowrap border-b-2 px-4 py-2.5 text-sm font-semibold transition',
                current === section
                  ? 'border-forest-600 text-forest-700'
                  : 'border-transparent text-muted hover:text-charcoal',
              )}
            >
              {ABOUT_SECTION_LABELS[section]}
            </button>
          ))}
        </div>
      )}

      {/* One template for all three tabs: words on the left, pictures on the
          right. Only the content differs between them — the camp adds its
          amenities above its photos, and its address below its text. */}
      <div role="tabpanel" id={`about-panel-${current}`} aria-labelledby={`about-tab-${current}`}>
        <div className={cx('mt-6 grid gap-8', hasRightColumn && 'lg:grid-cols-2')}>
          <div className="space-y-6">
            {body && (
              <p className="max-w-prose whitespace-pre-wrap text-base leading-relaxed text-muted">
                {body}
              </p>
            )}
            {current === 'camp' && <Directions address={content?.camp_address ?? null} />}
          </div>

          {hasRightColumn && (
            <div className="space-y-6">
              {/* Amenities describe the camp itself, so they stay on its tab
                  rather than repeating under the town's history. */}
              {current === 'camp' &&
                (isLoading ? (
                  <div className="flex justify-center py-10">
                    <Spinner />
                  </div>
                ) : amenities.length === 0 ? (
                  <EmptyState
                    title="No amenities listed yet"
                    hint="Add some from the admin panel's Site Content tab."
                  />
                ) : (
                  <div className="grid gap-3 sm:grid-cols-2">
                    {amenities.map((a) => {
                      const Icon = amenityIcon(a.icon);
                      return (
                        <Card key={a.id} className="flex items-center gap-3">
                          <Icon className="shrink-0 text-forest-600" size={13} />
                          <p className="font-semibold text-charcoal">{a.label}</p>
                        </Card>
                      );
                    })}
                  </div>
                ))}

              {!galleryBelow && <PhotoGrid photos={photos} onOpen={setLightbox} showEmpty={false} />}
            </div>
          )}
        </div>

        {/* The camp's own gallery, full width beneath the row above, with the
            shared album linked under it — the public pictures first, then the
            one that needs an account. Inside the panel, since it is this
            tab's content and changes with it. */}
        {galleryBelow && (
          <div className="mt-8">
            <PhotoGrid
              photos={photos}
              onOpen={setLightbox}
              // "Photos coming soon" belongs on the camp's own tab; the history
              // tabs are prose first and carry no grid until they have one.
              showEmpty={!isLoading}
              wide
            />
            <GroupPhotosLink />
          </div>
        )}
      </div>

      {/* Scoped to the visible tab: prev/next can only reach photos the
          reader is already looking at. */}
      <Lightbox
        photos={photos}
        index={lightbox}
        onClose={() => setLightbox(null)}
        onNavigate={setLightbox}
      />
    </section>
  );
}
