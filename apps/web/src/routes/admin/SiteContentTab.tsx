import { useEffect, useRef, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { ArrowDown, ArrowUp, ExternalLink, ImagePlus, Plus, Trash2, Upload } from 'lucide-react';
import { Button, Card, EmptyState, Field, Input, Section, Spinner, Textarea, cx } from '../../components/ui';
import { api, ApiError, assetUrl } from '../../lib/api';
import { AMENITY_ICON_NAMES, amenityIcon } from '../../lib/icons';
import {
  useAmenitiesAdmin,
  useGalleryAdmin,
  useRulesAdmin,
  useSiteSettingsAdmin,
} from '../../lib/queries';
import type {
  AboutSection,
  AmenityItem,
  GalleryPhoto,
  RuleItem,
  SiteSettingsAdmin,
} from '../../lib/types';
import { ABOUT_SECTIONS, ABOUT_SECTION_LABELS } from '../../lib/types';

const onError = (err: unknown) =>
  toast.error(err instanceof ApiError ? err.message : 'That action failed.');

// ─────────────────────────── hero / about / guest photos ───────────────────────────

function SettingsSection({ content }: { content: SiteSettingsAdmin | undefined }) {
  const queryClient = useQueryClient();
  const invalidate = () => {
    // Both: the landing page reads the public payload, this form reads the
    // admin-only one that still carries the album link.
    void queryClient.invalidateQueries({ queryKey: ['site-content'] });
    void queryClient.invalidateQueries({ queryKey: ['admin-site-settings'] });
  };
  const fileInput = useRef<HTMLInputElement>(null);

  const [form, setForm] = useState({
    hero_title: '',
    hero_subtitle: '',
    about_camp_text: '',
    about_dulac_text: '',
    last_island_text: '',
    camp_address: '',
    guest_photos_url: '',
  });

  // Seed the form once real data arrives, without clobbering in-progress edits.
  const [seeded, setSeeded] = useState(false);
  useEffect(() => {
    if (!content || seeded) return;
    setForm({
      hero_title: content.hero_title,
      hero_subtitle: content.hero_subtitle,
      about_camp_text: content.about_camp_text,
      about_dulac_text: content.about_dulac_text,
      last_island_text: content.last_island_text,
      camp_address: content.camp_address ?? '',
      guest_photos_url: content.guest_photos_url ?? '',
    });
    setSeeded(true);
  }, [content, seeded]);

  const save = useMutation({
    mutationFn: (body: typeof form) =>
      api('/admin/site-content/settings', { method: 'PUT', body }),
    onSuccess: () => {
      toast.success('Saved.');
      invalidate();
    },
    onError,
  });

  const uploadHero = useMutation({
    mutationFn: (file: File) => {
      const fd = new FormData();
      fd.append('file', file);
      return api<{ hero_image_url: string }>('/admin/site-content/hero-image', {
        method: 'POST',
        body: fd,
      });
    },
    onSuccess: () => {
      toast.success('Hero image updated.');
      invalidate();
    },
    onError,
  });

  if (!content) {
    return (
      <div className="flex justify-center py-10">
        <Spinner />
      </div>
    );
  }

  const heroImageUrl = assetUrl(content.hero_image_url);

  return (
    <div className="space-y-6">
      <Card>
        <h3 className="mb-4 font-bold text-charcoal">Hero</h3>
        <div className="grid gap-4 sm:grid-cols-[1fr_auto]">
          <div className="space-y-4">
            <Field label="Title">
              <Input
                value={form.hero_title}
                onChange={(e) => setForm((f) => ({ ...f, hero_title: e.target.value }))}
              />
            </Field>
            <Field label="Subtitle">
              <Input
                value={form.hero_subtitle}
                onChange={(e) => setForm((f) => ({ ...f, hero_subtitle: e.target.value }))}
              />
            </Field>
          </div>

          <div className="w-full sm:w-48">
            <span className="mb-1.5 block text-sm font-semibold text-charcoal">Image</span>
            <div className="relative flex aspect-video items-center justify-center overflow-hidden rounded-lg border border-sand bg-cream-dark">
              {heroImageUrl ? (
                <img src={heroImageUrl} alt="" className="h-full w-full object-cover" />
              ) : (
                <ImagePlus className="text-muted" size={24} />
              )}
              {uploadHero.isPending && (
                <div className="absolute inset-0 flex items-center justify-center bg-charcoal/50">
                  <Spinner className="border-cream border-t-forest-300" />
                </div>
              )}
            </div>
            <input
              ref={fileInput}
              type="file"
              accept="image/jpeg,image/png,image/webp"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) uploadHero.mutate(file);
                e.target.value = '';
              }}
            />
            <Button
              variant="ghost"
              size="sm"
              className="mt-2 w-full"
              disabled={uploadHero.isPending}
              onClick={() => fileInput.current?.click()}
            >
              <Upload size={14} /> {heroImageUrl ? 'Replace Image' : 'Upload Image'}
            </Button>
          </div>
        </div>

        <div className="mt-4 flex justify-end">
          <Button size="sm" disabled={save.isPending} onClick={() => save.mutate(form)}>
            {save.isPending ? 'Saving…' : 'Save hero'}
          </Button>
        </div>
      </Card>

      {/* Three separate stories, each its own section on the landing page.
          Each card saves on its own, matching the hero/guest-photos cards. */}
      <Card>
        <h3 className="mb-4 font-bold text-charcoal">About the Camp</h3>
        <Field
          label="About the camp"
          hint="The camp itself — what it is, who it's for, any background worth knowing."
        >
          <Textarea
            rows={5}
            value={form.about_camp_text}
            onChange={(e) => setForm((f) => ({ ...f, about_camp_text: e.target.value }))}
          />
        </Field>
        <div className="mt-4">
          <Field
            label="Camp address"
            hint="A street address, or just coordinates like '29.3802, -90.7148' if there's no clean mailing address for this location. Shown with a Get Directions button in this section. Leave blank to show neither."
          >
            <Input
              value={form.camp_address}
              placeholder="29.3802, -90.7148"
              onChange={(e) => setForm((f) => ({ ...f, camp_address: e.target.value }))}
            />
          </Field>
        </div>
        <div className="mt-4 flex justify-end">
          <Button size="sm" disabled={save.isPending} onClick={() => save.mutate(form)}>
            {save.isPending ? 'Saving…' : 'Save About the Camp'}
          </Button>
        </div>
      </Card>

      <Card>
        <h3 className="mb-4 font-bold text-charcoal">About Dulac</h3>
        <Field label="About Dulac" hint="A short history or story of the town itself.">
          <Textarea
            rows={5}
            value={form.about_dulac_text}
            onChange={(e) => setForm((f) => ({ ...f, about_dulac_text: e.target.value }))}
          />
        </Field>
        <div className="mt-4 flex justify-end">
          <Button size="sm" disabled={save.isPending} onClick={() => save.mutate(form)}>
            {save.isPending ? 'Saving…' : 'Save About Dulac'}
          </Button>
        </div>
      </Card>

      <Card>
        <h3 className="mb-4 font-bold text-charcoal">Last Island</h3>
        <Field
          label="Last Island"
          hint="The history of Last Island (Isle Dernière) and the 1856 hurricane."
        >
          <Textarea
            rows={5}
            value={form.last_island_text}
            onChange={(e) => setForm((f) => ({ ...f, last_island_text: e.target.value }))}
          />
        </Field>
        <div className="mt-4 flex justify-end">
          <Button size="sm" disabled={save.isPending} onClick={() => save.mutate(form)}>
            {save.isPending ? 'Saving…' : 'Save Last Island'}
          </Button>
        </div>
      </Card>

      <Card>
        <h3 className="mb-4 font-bold text-charcoal">Guest photos</h3>
        <Field
          label="Google Photos album link"
          hint="Paste the share link for a Google Photos album with “anyone with the link can add photos” turned on. Shown on My Stay to guests who have had a booking approved — not on the public site. Leave blank to hide it from them too."
        >
          <Input
            type="url"
            placeholder="https://photos.app.goo.gl/…"
            value={form.guest_photos_url}
            onChange={(e) => setForm((f) => ({ ...f, guest_photos_url: e.target.value }))}
          />
        </Field>
        {form.guest_photos_url && (
          <a
            href={form.guest_photos_url}
            target="_blank"
            rel="noreferrer noopener"
            className="mt-2 inline-flex items-center gap-1 text-xs font-semibold text-forest-700 hover:underline"
          >
            Preview link <ExternalLink size={12} />
          </a>
        )}
        <div className="mt-4 flex justify-end">
          <Button size="sm" disabled={save.isPending} onClick={() => save.mutate(form)}>
            {save.isPending ? 'Saving…' : 'Save guest photos link'}
          </Button>
        </div>
      </Card>
    </div>
  );
}

// ─────────────────────────── rules ───────────────────────────

function RulesSection() {
  const { data, isLoading } = useRulesAdmin();
  const queryClient = useQueryClient();
  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ['admin-rules'] });
    void queryClient.invalidateQueries({ queryKey: ['site-content'] });
  };
  const [draft, setDraft] = useState('');

  const create = useMutation({
    mutationFn: (text: string) =>
      api<RuleItem>('/admin/rules', { method: 'POST', body: { text, sort_order: (data?.length ?? 0) } }),
    onSuccess: () => {
      setDraft('');
      invalidate();
    },
    onError,
  });

  const update = useMutation({
    mutationFn: (item: RuleItem) =>
      api<RuleItem>(`/admin/rules/${item.id}`, {
        method: 'PUT',
        body: { text: item.text, sort_order: item.sort_order },
      }),
    onSuccess: invalidate,
    onError,
  });

  const remove = useMutation({
    mutationFn: (id: string) => api(`/admin/rules/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      toast.success('Rule removed.');
      invalidate();
    },
    onError,
  });

  const rules = data ?? [];

  const move = (index: number, direction: -1 | 1) => {
    const other = rules[index + direction];
    const mover = rules[index];
    if (!other) return;
    update.mutate({ ...mover, sort_order: other.sort_order });
    update.mutate({ ...other, sort_order: mover.sort_order });
  };

  if (isLoading) {
    return (
      <div className="flex justify-center py-10">
        <Spinner />
      </div>
    );
  }

  return (
    <Card>
      {rules.length === 0 ? (
        <EmptyState title="No house rules yet" hint="Add the first one below." />
      ) : (
        <ul className="mb-4 space-y-2">
          {rules.map((rule, i) => (
            <li key={rule.id} className="flex items-center gap-2">
              <div className="flex flex-col">
                <button
                  disabled={i === 0}
                  onClick={() => move(i, -1)}
                  className="text-muted hover:text-charcoal disabled:opacity-25"
                  aria-label="Move up"
                >
                  <ArrowUp size={14} />
                </button>
                <button
                  disabled={i === rules.length - 1}
                  onClick={() => move(i, 1)}
                  className="text-muted hover:text-charcoal disabled:opacity-25"
                  aria-label="Move down"
                >
                  <ArrowDown size={14} />
                </button>
              </div>
              <Input
                defaultValue={rule.text}
                onBlur={(e) => {
                  const text = e.target.value.trim();
                  if (text && text !== rule.text) update.mutate({ ...rule, text });
                }}
              />
              <button
                onClick={() => remove.mutate(rule.id)}
                title="Delete"
                className="shrink-0 rounded p-2 text-muted hover:bg-cream-dark hover:text-clay"
              >
                <Trash2 size={16} />
              </button>
            </li>
          ))}
        </ul>
      )}

      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (draft.trim()) create.mutate(draft.trim());
        }}
      >
        <Input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Add a house rule…"
        />
        <Button type="submit" size="sm" disabled={create.isPending || !draft.trim()}>
          <Plus size={14} /> Add
        </Button>
      </form>
    </Card>
  );
}

// ─────────────────────────── amenities ───────────────────────────

function AmenitiesSection() {
  const { data, isLoading } = useAmenitiesAdmin();
  const queryClient = useQueryClient();
  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ['admin-amenities'] });
    void queryClient.invalidateQueries({ queryKey: ['site-content'] });
  };
  const [draft, setDraft] = useState({ label: '', icon: '' });

  const create = useMutation({
    mutationFn: (body: { label: string; icon: string | null }) =>
      api<AmenityItem>('/admin/amenities', {
        method: 'POST',
        body: { ...body, sort_order: data?.length ?? 0 },
      }),
    onSuccess: () => {
      setDraft({ label: '', icon: '' });
      invalidate();
    },
    onError,
  });

  const update = useMutation({
    mutationFn: (item: AmenityItem) =>
      api<AmenityItem>(`/admin/amenities/${item.id}`, {
        method: 'PUT',
        body: { label: item.label, icon: item.icon, sort_order: item.sort_order },
      }),
    onSuccess: invalidate,
    onError,
  });

  const remove = useMutation({
    mutationFn: (id: string) => api(`/admin/amenities/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      toast.success('Amenity removed.');
      invalidate();
    },
    onError,
  });

  const amenities = data ?? [];

  const move = (index: number, direction: -1 | 1) => {
    const other = amenities[index + direction];
    const mover = amenities[index];
    if (!other) return;
    update.mutate({ ...mover, sort_order: other.sort_order });
    update.mutate({ ...other, sort_order: mover.sort_order });
  };

  if (isLoading) {
    return (
      <div className="flex justify-center py-10">
        <Spinner />
      </div>
    );
  }

  const DraftIcon = amenityIcon(draft.icon);

  return (
    <Card>
      {amenities.length === 0 ? (
        <EmptyState title="No amenities yet" hint="Add the first one below." />
      ) : (
        <ul className="mb-4 space-y-2">
          {amenities.map((a, i) => {
            const Icon = amenityIcon(a.icon);
            return (
              <li key={a.id} className="flex items-center gap-2">
                <div className="flex flex-col">
                  <button
                    disabled={i === 0}
                    onClick={() => move(i, -1)}
                    className="text-muted hover:text-charcoal disabled:opacity-25"
                    aria-label="Move up"
                  >
                    <ArrowUp size={14} />
                  </button>
                  <button
                    disabled={i === amenities.length - 1}
                    onClick={() => move(i, 1)}
                    className="text-muted hover:text-charcoal disabled:opacity-25"
                    aria-label="Move down"
                  >
                    <ArrowDown size={14} />
                  </button>
                </div>
                <Icon size={18} className="shrink-0 text-forest-600" />
                <Input
                  defaultValue={a.label}
                  className="flex-1"
                  onBlur={(e) => {
                    const label = e.target.value.trim();
                    if (label && label !== a.label) update.mutate({ ...a, label });
                  }}
                />
                <div className="w-36">
                  <Input
                    defaultValue={a.icon ?? ''}
                    list="amenity-icon-names"
                    placeholder="Icon name"
                    onBlur={(e) => {
                      const icon = e.target.value.trim() || null;
                      if (icon !== a.icon) update.mutate({ ...a, icon });
                    }}
                  />
                </div>
                <button
                  onClick={() => remove.mutate(a.id)}
                  title="Delete"
                  className="shrink-0 rounded p-2 text-muted hover:bg-cream-dark hover:text-clay"
                >
                  <Trash2 size={16} />
                </button>
              </li>
            );
          })}
        </ul>
      )}

      <datalist id="amenity-icon-names">
        {AMENITY_ICON_NAMES.map((n) => (
          <option key={n} value={n} />
        ))}
      </datalist>

      <form
        className="flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (draft.label.trim()) create.mutate({ label: draft.label.trim(), icon: draft.icon.trim() || null });
        }}
      >
        <DraftIcon size={18} className="shrink-0 text-muted" />
        <Input
          value={draft.label}
          onChange={(e) => setDraft((d) => ({ ...d, label: e.target.value }))}
          placeholder="Amenity, e.g. Wi-Fi & TV"
          className="flex-1"
        />
        <div className="w-36">
          <Input
            value={draft.icon}
            onChange={(e) => setDraft((d) => ({ ...d, icon: e.target.value }))}
            list="amenity-icon-names"
            placeholder="Icon name"
          />
        </div>
        <Button type="submit" size="sm" disabled={create.isPending || !draft.label.trim()}>
          <Plus size={14} /> Add
        </Button>
      </form>
    </Card>
  );
}

// ─────────────────────────── gallery ───────────────────────────

/**
 * One About section's photos. Rendered once per section, so which section an
 * upload belongs to is implicit in which drop zone it was dropped on — no
 * extra picker, and it matches how the single gallery already worked.
 */
function GallerySection({ section }: { section: AboutSection }) {
  const { data, isLoading } = useGalleryAdmin();
  const queryClient = useQueryClient();
  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ['admin-gallery'] });
    void queryClient.invalidateQueries({ queryKey: ['site-content'] });
  };
  const fileInput = useRef<HTMLInputElement>(null);

  const upload = useMutation({
    mutationFn: (file: File) => {
      const fd = new FormData();
      fd.append('file', file);
      fd.append('about_section', section);
      return api<GalleryPhoto>('/admin/gallery', { method: 'POST', body: fd });
    },
    onSuccess: () => {
      toast.success('Photo added.');
      invalidate();
    },
    onError,
  });

  const update = useMutation({
    mutationFn: (item: GalleryPhoto) =>
      api<GalleryPhoto>(`/admin/gallery/${item.id}`, {
        method: 'PUT',
        body: { caption: item.caption, sort_order: item.sort_order },
      }),
    onSuccess: invalidate,
    onError,
  });

  const remove = useMutation({
    mutationFn: (id: string) => api(`/admin/gallery/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      toast.success('Photo removed.');
      invalidate();
    },
    onError,
  });

  // Only this section's photos — reordering and removal stay inside it.
  const photos = (data ?? []).filter((p) => p.about_section === section);

  const move = (index: number, direction: -1 | 1) => {
    const other = photos[index + direction];
    const mover = photos[index];
    if (!other) return;
    update.mutate({ ...mover, sort_order: other.sort_order });
    update.mutate({ ...other, sort_order: mover.sort_order });
  };

  if (isLoading) {
    return (
      <div className="flex justify-center py-10">
        <Spinner />
      </div>
    );
  }

  return (
    <Card>
      <div
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          e.preventDefault();
          const file = e.dataTransfer.files?.[0];
          if (file) upload.mutate(file);
        }}
        onClick={() => fileInput.current?.click()}
        className="mb-4 flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-sand bg-cream-dark/40 px-6 py-8 text-center text-muted transition hover:border-forest-400 hover:text-forest-700"
      >
        {upload.isPending ? <Spinner /> : <ImagePlus size={24} />}
        <p className="text-sm font-semibold">
          {upload.isPending ? 'Uploading…' : 'Click or drag a photo here to add it'}
        </p>
        <p className="text-xs">JPG, PNG, or WEBP — up to 8MB</p>
      </div>
      <input
        ref={fileInput}
        type="file"
        accept="image/jpeg,image/png,image/webp"
        className="hidden"
        onChange={(e) => {
          const file = e.target.files?.[0];
          if (file) upload.mutate(file);
          e.target.value = '';
        }}
      />

      {photos.length === 0 ? (
        <EmptyState title="No photos yet" hint="Add one above." />
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {photos.map((p, i) => (
            <div key={p.id} className="overflow-hidden rounded-lg border border-sand bg-white">
              <div className="aspect-4/3 bg-cream-dark">
                <img src={assetUrl(p.url) ?? undefined} alt={p.caption ?? ''} className="h-full w-full object-cover" />
              </div>
              <div className="space-y-2 p-2">
                <Input
                  defaultValue={p.caption ?? ''}
                  placeholder="Caption (optional)"
                  className="text-xs"
                  onBlur={(e) => {
                    const caption = e.target.value.trim() || null;
                    if (caption !== p.caption) update.mutate({ ...p, caption });
                  }}
                />
                <div className="flex items-center justify-between">
                  <div className="flex gap-1">
                    <button
                      disabled={i === 0}
                      onClick={() => move(i, -1)}
                      className="rounded p-1.5 text-muted hover:bg-cream-dark hover:text-charcoal disabled:opacity-25"
                      aria-label="Move earlier"
                    >
                      <ArrowUp size={14} />
                    </button>
                    <button
                      disabled={i === photos.length - 1}
                      onClick={() => move(i, 1)}
                      className="rounded p-1.5 text-muted hover:bg-cream-dark hover:text-charcoal disabled:opacity-25"
                      aria-label="Move later"
                    >
                      <ArrowDown size={14} />
                    </button>
                  </div>
                  <button
                    onClick={() => remove.mutate(p.id)}
                    title="Delete"
                    className={cx('rounded p-1.5 text-muted hover:bg-cream-dark hover:text-clay')}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}

// ─────────────────────────── tab ───────────────────────────

export default function SiteContentTab() {
  const { data: content } = useSiteSettingsAdmin();

  return (
    <div className="space-y-10">
      <Section
        title="Hero, about & guest photos"
        subtitle="The top of the public site, plus the album link only past and upcoming guests see."
      >
        <SettingsSection content={content} />
      </Section>
      <Section title="House rules" subtitle="Shown on the landing page, in order.">
        <RulesSection />
      </Section>
      <Section title="Amenities" subtitle="Shown on the landing page, in order.">
        <AmenitiesSection />
      </Section>
      {/* One gallery per About tab. Photos that predate the split were
          migrated to About the Camp, which is where they already appeared. */}
      {ABOUT_SECTIONS.map((section) => (
        <Section
          key={section}
          title={`${ABOUT_SECTION_LABELS[section]} — photos`}
          subtitle="Shown on this section's tab on the landing page, in order."
        >
          <GallerySection section={section} />
        </Section>
      ))}
    </div>
  );
}
