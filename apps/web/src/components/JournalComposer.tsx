import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { CheckCircle2, RotateCcw, Send, TriangleAlert } from 'lucide-react';
import { Button, Field, Input, Spinner, Textarea } from './ui';
import { CatchLog } from './CatchLog';
import { DraftPhotoGallery, DraftPhotoImg } from './JournalPhotos';
import { JournalVisibilityChoice } from './JournalVisibility';
import { ApiError } from '../lib/api';
import { useFishSpecies } from '../lib/queries';
import { useLeaveGuard } from '../lib/useLeaveGuard';
import {
  MAX_PHOTOS,
  applyPhotoChanges,
  draftFrom,
  fingerprint,
  payloadOf,
  photoCount,
  resolveUploads,
  screenFiles,
  type Draft,
  type DraftSource,
  type PhotoFailure,
} from '../lib/journalDraft';
import type { JournalCatch } from '../lib/types';

/** What a successful POST/PUT hands back — enough to attach photos to. */
export interface SavedEntry {
  id: string;
  catches: JournalCatch[];
}

type Phase =
  | { name: 'composing' }
  | { name: 'submitting'; progress: string }
  | { name: 'photos-failed'; entryId: string; failures: PhotoFailure[]; retrying: boolean };

/**
 * The whole journal entry form — story, catch log, and photos — composed in
 * the browser and sent in one go.
 *
 * One model for both creating and editing: nothing reaches the server until
 * the submit button, and Cancel is just "forget all of this". Under the hood
 * a submit is the entry write (POST or PUT, catches included) followed by
 * the photo changes; to the person using it, it is one button and one
 * outcome.
 *
 * If the entry saves but some photos don't, the story is *not* at risk — it
 * is already saved — so rather than an error the form turns into a short
 * list of the photos that didn't make it, with a retry for just those.
 */
export function JournalComposer({
  source,
  save,
  submitLabel,
  submittingLabel,
  onDone,
  onCancel,
  onDirtyChange,
  visibilityName,
  guardNavigation = true,
}: {
  /** The saved entry to edit; absent for a new story. */
  source?: DraftSource;
  /** Writes the entry (POST or PUT) and returns it as saved. */
  save: (payload: ReturnType<typeof payloadOf>) => Promise<SavedEntry>;
  submitLabel: string;
  submittingLabel: string;
  /** Everything is saved, or the author chose to go on without failed photos. */
  onDone: (outcome: { skippedPhotos: number }) => void;
  onCancel: () => void;
  /** For a host (a modal) that needs to know whether closing loses work. */
  onDirtyChange?: (dirty: boolean) => void;
  visibilityName?: string;
  /** Off for a host that can't be navigated away from on its own. */
  guardNavigation?: boolean;
}) {
  const { data: species } = useFishSpecies();
  const [initial] = useState(() => draftFrom(source));
  const [draft, setDraft] = useState<Draft>(initial);
  const [baseline] = useState(() => fingerprint(initial));
  const [phase, setPhase] = useState<Phase>({ name: 'composing' });
  // Set just before a deliberate exit, so the leave guard steps aside for it.
  // A ref, not state: the host navigates in the same tick, before any
  // re-render could deliver a state change to the guard.
  const leaving = useRef(false);

  const dirty = fingerprint(draft) !== baseline;
  // Unsaved work is at risk while composing; while submitting and with
  // failed photos outstanding, so are the photos not yet uploaded.
  const atRisk = phase.name !== 'composing' || dirty;

  useEffect(() => onDirtyChange?.(atRisk), [atRisk, onDirtyChange]);
  useLeaveGuard(() => guardNavigation && !leaving.current && atRisk);

  const count = photoCount(draft);
  const atCap = count >= MAX_PHOTOS;
  const set = (changes: Partial<Draft>) => setDraft((d) => ({ ...d, ...changes }));

  const stage = (files: File[]) => {
    const { accepted, problems } = screenFiles(files, MAX_PHOTOS - count);
    problems.forEach((p) => toast.error(p));
    return accepted;
  };

  const speciesName = (id: string | null) =>
    (id && species?.find((s) => s.id === id)?.name) || null;

  const finish = (skippedPhotos: number) => {
    leaving.current = true;
    onDone({ skippedPhotos });
  };

  const submit = async () => {
    setPhase({ name: 'submitting', progress: 'Saving your story…' });
    let saved: SavedEntry;
    try {
      saved = await save(payloadOf(draft));
    } catch (err) {
      // Nothing was saved, so nothing is lost: back to the form as it was.
      toast.error(err instanceof ApiError ? err.message : 'Could not save your story. Please try again.');
      setPhase({ name: 'composing' });
      return;
    }

    const { uploads, unmatched } = resolveUploads(draft, saved.catches, speciesName);
    const failures = [
      ...unmatched,
      ...(await applyPhotoChanges(saved.id, draft.removedPhotoIds, uploads, (progress) =>
        setPhase({ name: 'submitting', progress }),
      )),
    ];
    if (failures.length === 0) finish(0);
    else setPhase({ name: 'photos-failed', entryId: saved.id, failures, retrying: false });
  };

  const retry = async () => {
    if (phase.name !== 'photos-failed') return;
    setPhase({ ...phase, retrying: true });
    const removals = phase.failures.flatMap((f) => (f.kind === 'remove' ? [f.photoId] : []));
    const uploads = phase.failures.flatMap((f) => (f.kind === 'upload' && f.retryable ? [f.upload] : []));
    // A photo whose catch couldn't be found can't be retried into it — it
    // stays listed rather than being quietly filed somewhere else.
    const stuck = phase.failures.filter((f) => f.kind === 'upload' && !f.retryable);
    const failures = [...stuck, ...(await applyPhotoChanges(phase.entryId, removals, uploads, () => {}))];
    if (failures.length === 0) {
      toast.success('All photos attached.');
      finish(0);
    } else {
      setPhase({ ...phase, failures, retrying: false });
    }
  };

  // ── some photos didn't make it ──
  if (phase.name === 'photos-failed') {
    const n = phase.failures.length;
    return (
      <div className="space-y-4">
        <p className="flex items-start gap-2 rounded-lg border border-forest-300 bg-forest-50 px-3 py-2.5 text-sm text-forest-900">
          <CheckCircle2 size={16} className="mt-0.5 shrink-0" />
          <span>Your story and catch log are saved.</span>
        </p>
        <div className="rounded-lg border border-amber-300 bg-amber-50 px-3 py-3 text-sm text-amber-900">
          <p className="flex items-start gap-2 font-semibold">
            <TriangleAlert size={16} className="mt-0.5 shrink-0" />
            {n === 1 ? '1 photo change' : `${n} photo changes`} didn't go through:
          </p>
          <ul className="mt-3 space-y-2">
            {phase.failures.map((f) => (
              <li key={f.kind === 'upload' ? f.upload.key : f.photoId} className="flex items-center gap-3">
                {f.kind === 'upload' ? (
                  <DraftPhotoImg
                    item={{ kind: 'staged', staged: { key: f.upload.key, file: f.upload.file } }}
                    className="h-12 w-12 shrink-0 rounded-md border border-amber-200 object-cover"
                  />
                ) : (
                  <div className="h-12 w-12 shrink-0 rounded-md border border-amber-200 bg-white" />
                )}
                <div className="min-w-0 text-xs">
                  <p className="font-semibold">
                    {f.kind === 'remove'
                      ? 'Removing a photo'
                      : f.upload.catchLabel
                        ? `Photo for ${f.upload.catchLabel}`
                        : 'Story photo'}
                  </p>
                  <p>{f.message}</p>
                </div>
              </li>
            ))}
          </ul>
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <Button type="button" variant="ghost" disabled={phase.retrying} onClick={() => finish(n)}>
            Continue without them
          </Button>
          <Button
            type="button"
            disabled={phase.retrying || !phase.failures.some((f) => f.kind === 'remove' || f.retryable)}
            onClick={retry}
          >
            {phase.retrying ? (
              <>
                <Spinner className="h-4 w-4 border-2" /> Trying again…
              </>
            ) : (
              <>
                <RotateCcw size={16} /> Try again
              </>
            )}
          </Button>
        </div>
      </div>
    );
  }

  // ── composing / submitting ──
  const submitting = phase.name === 'submitting';
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      {/* A disabled fieldset freezes every control inside while submitting. */}
      <fieldset disabled={submitting} className="space-y-4">
        <Field label="Title">
          <Input
            required
            value={draft.title}
            onChange={(e) => set({ title: e.target.value })}
            placeholder="Redfish on the falling tide"
          />
        </Field>
        <Field label="Story">
          <Textarea
            required
            rows={9}
            value={draft.body}
            onChange={(e) => set({ body: e.target.value })}
            placeholder="Tell us about your stay…"
          />
        </Field>
        <DraftPhotoGallery
          saved={draft.photos}
          staged={draft.staged}
          atCap={atCap}
          onPick={(files) => set({ staged: [...draft.staged, ...stage(files)] })}
          onRemoveSaved={(id) =>
            set({
              photos: draft.photos.filter((p) => p.id !== id),
              removedPhotoIds: [...draft.removedPhotoIds, id],
            })
          }
          onRemoveStaged={(key) => set({ staged: draft.staged.filter((s) => s.key !== key) })}
        />
        <CatchLog
          catches={draft.catches}
          photosAtCap={atCap}
          onChange={(catches) =>
            setDraft((d) => {
              // A saved photo removed from a row that is *kept* has to be
              // deleted explicitly. One on a row removed outright doesn't —
              // the row's delete cascades it on save.
              const removed = d.catches.flatMap((before) => {
                const after = catches.find((c) => c.key === before.key);
                return after
                  ? before.photos.filter((p) => !after.photos.some((q) => q.id === p.id)).map((p) => p.id)
                  : [];
              });
              return { ...d, catches, removedPhotoIds: [...d.removedPhotoIds, ...removed] };
            })
          }
          onPickPhotos={(key, files) => {
            const accepted = stage(files);
            setDraft((d) => ({
              ...d,
              catches: d.catches.map((c) => (c.key === key ? { ...c, staged: [...c.staged, ...accepted] } : c)),
            }));
          }}
        />
        <JournalVisibilityChoice
          value={draft.visibility}
          onChange={(visibility) => set({ visibility })}
          name={visibilityName}
        />
      </fieldset>

      <div className="mt-5 flex flex-wrap items-center justify-end gap-2">
        {submitting && (
          <p className="mr-auto flex items-center gap-2 text-sm text-muted" aria-live="polite">
            <Spinner className="h-4 w-4 border-2" /> {phase.progress}
          </p>
        )}
        <Button
          type="button"
          variant="ghost"
          disabled={submitting}
          onClick={() => {
            // Deliberate, so no "are you sure" — nothing was ever sent.
            leaving.current = true;
            onCancel();
          }}
        >
          Cancel
        </Button>
        <Button type="submit" size="lg" disabled={submitting}>
          <Send size={16} /> {submitting ? submittingLabel : submitLabel}
        </Button>
      </div>
    </form>
  );
}
