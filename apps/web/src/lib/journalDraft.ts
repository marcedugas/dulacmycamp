import { api, ApiError } from './api';
import type { JournalCatch, JournalPhoto, JournalVisibility } from './types';

/**
 * A journal entry being composed, held entirely in the browser.
 *
 * Nothing here touches the server until the form is submitted (see
 * `JournalComposer`, which drives {@link applyPhotoChanges}): catch rows,
 * photos (new ones as bare `File`s), and removals of photos that already
 * exist are all just state. That is what lets Cancel be a no-op on the
 * server, and what lets a first-time story carry per-catch photos at all —
 * the catch rows they belong to don't exist until the story is posted.
 */

/** Mirrors `MAX_CATCHES_PER_ENTRY` in `services/api/src/journal.rs`. */
export const MAX_CATCHES = 25;
/** Mirrors `MAX_PHOTOS_PER_ENTRY` in `services/api/src/journal.rs`. */
export const MAX_PHOTOS = 20;
/** Mirrors `MAX_IMAGE_BYTES` in `services/api/src/uploads.rs`. */
export const MAX_PHOTO_MB = 20;
export const PHOTO_TYPES = ['image/jpeg', 'image/png', 'image/webp'];

/** A photo picked but not uploaded yet. */
export interface StagedPhoto {
  key: string;
  file: File;
}

export interface DraftCatch {
  /** Stable React key; also survives while `id` is still null. */
  key: string;
  /** The saved row's id, or null for a row that exists only here so far. */
  id: string | null;
  species_id: string | null;
  length_inches: number | null;
  weight_lbs: number | null;
  notes: string | null;
  /** Already uploaded and still kept. */
  photos: JournalPhoto[];
  staged: StagedPhoto[];
}

export interface Draft {
  title: string;
  body: string;
  visibility: JournalVisibility;
  catches: DraftCatch[];
  /** General (gallery) photos already uploaded and still kept. */
  photos: JournalPhoto[];
  staged: StagedPhoto[];
  /** Saved photos the author removed — deleted from the server on submit. */
  removedPhotoIds: string[];
}

/** Either kind of photo a draft holds, for the pieces that show both alike. */
export type DraftPhotoItem =
  | { kind: 'saved'; photo: JournalPhoto }
  | { kind: 'staged'; staged: StagedPhoto };

export const draftPhotoItems = (saved: JournalPhoto[], staged: StagedPhoto[]): DraftPhotoItem[] => [
  ...saved.map((photo) => ({ kind: 'saved' as const, photo })),
  ...staged.map((s) => ({ kind: 'staged' as const, staged: s })),
];

/** The fields of a saved entry the composer seeds itself from. */
export interface DraftSource {
  title: string;
  body: string;
  visibility: JournalVisibility;
  catches: JournalCatch[];
  photos: JournalPhoto[];
}

// A counter rather than crypto.randomUUID(): the latter only exists in a
// secure context, and a phone testing against a LAN address isn't one.
let seq = 0;
export const newKey = () => `draft-${++seq}`;

export const emptyCatch = (): DraftCatch => ({
  key: newKey(),
  id: null,
  species_id: null,
  length_inches: null,
  weight_lbs: null,
  notes: null,
  photos: [],
  staged: [],
});

export function draftFrom(source?: DraftSource): Draft {
  return {
    title: source?.title ?? '',
    body: source?.body ?? '',
    // Family by default — the private option, matching the column default.
    visibility: source?.visibility ?? 'family',
    catches: (source?.catches ?? []).map((c) => ({
      key: newKey(),
      id: c.id,
      species_id: c.species_id,
      length_inches: c.length_inches,
      weight_lbs: c.weight_lbs,
      notes: c.notes,
      photos: c.photos,
      staged: [],
    })),
    photos: source?.photos ?? [],
    staged: [],
    removedPhotoIds: [],
  };
}

/**
 * Every photo the entry will hold once submitted. A catch row that has been
 * removed no longer counts: its photos go with it on save.
 */
export const photoCount = (d: Draft) =>
  d.photos.length +
  d.staged.length +
  d.catches.reduce((n, c) => n + c.photos.length + c.staged.length, 0);

/**
 * Everything a submit would change, as a comparable string — "is there
 * anything to lose?" is this differing from the starting point.
 */
export const fingerprint = (d: Draft) =>
  JSON.stringify([
    d.title,
    d.body,
    d.visibility,
    d.catches.map((c) => [
      c.id,
      c.species_id,
      c.length_inches,
      c.weight_lbs,
      c.notes ?? '',
      c.photos.map((p) => p.id),
      c.staged.map((s) => s.key),
    ]),
    d.photos.map((p) => p.id),
    d.staged.map((s) => s.key),
    d.removedPhotoIds,
  ]);

/**
 * Screens picked files before staging, so problems show while composing
 * rather than as a failed upload after Post. The server enforces all of
 * this again; this is only the early, friendly half.
 */
export function screenFiles(files: File[], room: number): { accepted: StagedPhoto[]; problems: string[] } {
  const accepted: StagedPhoto[] = [];
  const problems: string[] = [];
  let skippedForRoom = 0;
  for (const file of files) {
    if (!PHOTO_TYPES.includes(file.type)) {
      problems.push(`${file.name} isn't a JPG, PNG, or WEBP photo.`);
    } else if (file.size > MAX_PHOTO_MB * 1024 * 1024) {
      problems.push(`${file.name} is over ${MAX_PHOTO_MB}MB.`);
    } else if (accepted.length >= room) {
      skippedForRoom++;
    } else {
      accepted.push({ key: newKey(), file });
    }
  }
  if (skippedForRoom > 0) {
    problems.push(
      `A story can have up to ${MAX_PHOTOS} photos — ${skippedForRoom} ${skippedForRoom === 1 ? 'was' : 'were'} not added.`,
    );
  }
  return { accepted, problems };
}

/** What POST/PUT /journal take. Quantity is no longer collected. */
export const payloadOf = (d: Draft) => ({
  title: d.title.trim(),
  body: d.body.trim(),
  visibility: d.visibility,
  catches: d.catches.map((c) => ({
    id: c.id,
    species_id: c.species_id,
    length_inches: c.length_inches,
    weight_lbs: c.weight_lbs,
    notes: c.notes?.trim() || null,
  })),
});

/** One photo still to upload, now that it knows where it's going. */
export interface PendingUpload {
  key: string;
  file: File;
  /** The saved catch it attaches to; null for a general entry photo. */
  catchId: string | null;
  /** For telling the author which fish a failed photo was for. */
  catchLabel: string | null;
}

export type PhotoFailure =
  | {
      kind: 'upload';
      upload: PendingUpload;
      message: string;
      /** False when retrying can't help — the photo has nowhere to go. */
      retryable: boolean;
    }
  | { kind: 'remove'; photoId: string; message: string };

const messageOf = (err: unknown, fallback: string) =>
  err instanceof ApiError ? err.message : fallback;

/**
 * Matches each staged catch photo to the saved row it was attached to.
 *
 * The API stores each catch's `sort_order` as its exact index in the
 * submitted list (see `write_catches` in journal.rs), so submitted row `i` is
 * the saved catch with `sort_order === i` — a guarantee the server makes,
 * not an assumption about response order. A row that somehow can't be found
 * turns its photos into failures rather than guessing where they belong.
 */
export function resolveUploads(
  d: Draft,
  saved: JournalCatch[],
  speciesName: (id: string | null) => string | null,
): { uploads: PendingUpload[]; unmatched: PhotoFailure[] } {
  const uploads: PendingUpload[] = [];
  const unmatched: PhotoFailure[] = [];

  d.catches.forEach((c, i) => {
    const row = saved.find((s) => s.sort_order === i);
    const catchLabel = speciesName(c.species_id) ?? `Catch ${i + 1}`;
    for (const s of c.staged) {
      const upload = { key: s.key, file: s.file, catchId: row?.id ?? null, catchLabel };
      if (row) uploads.push(upload);
      else
        unmatched.push({
          kind: 'upload',
          upload,
          message: "Couldn't find this catch after saving, so this photo wasn't attached.",
          retryable: false,
        });
    }
  });
  for (const s of d.staged) {
    uploads.push({ key: s.key, file: s.file, catchId: null, catchLabel: null });
  }
  return { uploads, unmatched };
}

/**
 * Removes and uploads photos against a saved entry, one at a time, and
 * reports what didn't make it rather than stopping at the first failure.
 *
 * Removals first, so swapping one photo for another on an entry that is at
 * the cap doesn't bounce. Uploads are sequential: `sort_order` is assigned
 * server-side from the current max, so overlapping uploads would race.
 */
export async function applyPhotoChanges(
  entryId: string,
  removals: string[],
  uploads: PendingUpload[],
  onProgress: (text: string) => void,
): Promise<PhotoFailure[]> {
  const failures: PhotoFailure[] = [];

  if (removals.length > 0) onProgress('Removing photos…');
  for (const photoId of removals) {
    try {
      await api(`/journal/photos/${photoId}`, { method: 'DELETE' });
    } catch (err) {
      // Already gone is what we wanted anyway.
      if (err instanceof ApiError && err.status === 404) continue;
      failures.push({ kind: 'remove', photoId, message: messageOf(err, 'Could not remove this photo.') });
    }
  }

  for (const [i, u] of uploads.entries()) {
    onProgress(`Uploading photo ${i + 1} of ${uploads.length}…`);
    const fd = new FormData();
    fd.append('file', u.file);
    if (u.catchId) fd.append('journal_catch_id', u.catchId);
    try {
      await api<JournalPhoto>(`/journal/${entryId}/photos`, { method: 'POST', body: fd });
    } catch (err) {
      failures.push({
        kind: 'upload',
        upload: u,
        message: messageOf(err, 'Could not upload this photo.'),
        retryable: true,
      });
    }
  }
  return failures;
}
