import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Lock } from 'lucide-react';
import { Card, EmptyState, Spinner, cx } from '../../components/ui';
import { api, ApiError } from '../../lib/api';
import { useContentAccess } from '../../lib/queries';
import type { ContentAccessSection, Role } from '../../lib/types';
import { ROLE_LABELS, ROLES } from '../../lib/types';

/** A checkbox that reads as a pill, so a row of them scans as one setting. */
function Toggle({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <label
      className={cx(
        'inline-flex cursor-pointer items-center gap-2 rounded-full border px-3 py-1.5 text-xs font-semibold transition',
        disabled && 'cursor-not-allowed opacity-60',
        checked
          ? 'border-forest-300 bg-forest-100 text-forest-800'
          : 'border-sand bg-cream-dark text-muted',
      )}
    >
      <input
        type="checkbox"
        className="h-3.5 w-3.5 accent-forest-600"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      {label}
    </label>
  );
}

function SectionRow({ section }: { section: ContentAccessSection }) {
  const queryClient = useQueryClient();
  const save = useMutation({
    mutationFn: (body: { allowed_roles: Role[]; approved_booking_grants: boolean }) =>
      api(`/admin/content-access/${section.section_key}`, { method: 'PUT', body }),
    onSuccess: () => {
      toast.success(`${section.label} updated.`);
      void queryClient.invalidateQueries({ queryKey: ['admin-content-access'] });
      // The viewer's own access may have just changed.
      void queryClient.invalidateQueries({ queryKey: ['guest-photos-link'] });
    },
    onError: (err) =>
      toast.error(err instanceof ApiError ? err.message : 'Could not save that change.'),
  });

  const update = (patch: Partial<Pick<ContentAccessSection, 'allowed_roles' | 'approved_booking_grants'>>) =>
    save.mutate({
      allowed_roles: patch.allowed_roles ?? section.allowed_roles,
      approved_booking_grants: patch.approved_booking_grants ?? section.approved_booking_grants,
    });

  const toggleRole = (role: Role, on: boolean) =>
    update({
      allowed_roles: on
        ? [...section.allowed_roles, role]
        : section.allowed_roles.filter((r) => r !== role),
    });

  return (
    <Card>
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <h3 className="font-semibold text-charcoal">{section.label}</h3>
          <p className="mt-1 max-w-prose text-sm text-muted">{section.description}</p>
        </div>
        {!section.configurable && (
          <span
            className="inline-flex shrink-0 items-center gap-1 rounded-full border border-wood-300 bg-wood-100 px-2 py-0.5 text-xs font-semibold text-wood-800"
            title="Access depends on a booking, not on a role — so there is nothing here to toggle"
          >
            <Lock size={11} /> Booking-based
          </span>
        )}
      </div>

      {section.configurable && (
        <div className="mt-4 flex flex-wrap items-center gap-2">
          {ROLES.map((role) => (
            <Toggle
              key={role}
              label={ROLE_LABELS[role]}
              checked={section.allowed_roles.includes(role)}
              disabled={save.isPending}
              onChange={(on) => toggleRole(role, on)}
            />
          ))}
          <span className="mx-1 text-xs text-muted">or</span>
          <Toggle
            label="Anyone who has stayed"
            checked={section.approved_booking_grants}
            disabled={save.isPending}
            onChange={(on) => update({ approved_booking_grants: on })}
          />
          {save.isPending && <Spinner className="h-4 w-4" />}
        </div>
      )}
    </Card>
  );
}

export default function AccessTab() {
  const { data: sections, isLoading } = useContentAccess();

  if (isLoading) {
    return (
      <div className="flex justify-center py-10">
        <Spinner />
      </div>
    );
  }
  if (!sections || sections.length === 0) {
    return <EmptyState title="Nothing gated yet" />;
  }

  return (
    <div className="space-y-4">
      <p className="max-w-prose text-sm text-muted">
        Who can see each part of the site that isn&rsquo;t public. A section opens to anyone
        holding one of its ticked roles, <em>or</em> to anyone who has ever had a booking approved
        &mdash; whichever fits first. Changes take effect immediately, with no deploy.
      </p>
      {sections.map((s) => (
        <SectionRow key={s.section_key} section={s} />
      ))}
    </div>
  );
}
