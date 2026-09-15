import { Fish, Plus, Trash2 } from 'lucide-react';
import { Button, Input, cx } from './ui';
import { useFishSpecies } from '../lib/queries';
import type { JournalCatch, JournalCatchInput } from '../lib/types';

/** A blank row, so "add a catch" always starts from the same place. */
export const emptyCatch = (): JournalCatchInput => ({
  species_id: null,
  length_inches: null,
  weight_lbs: null,
  quantity: 1,
  notes: null,
});

/** Turns saved catches back into editable rows (ids are not resubmitted). */
export const toCatchInputs = (catches: JournalCatch[]): JournalCatchInput[] =>
  catches.map((c) => ({
    species_id: c.species_id,
    length_inches: c.length_inches,
    weight_lbs: c.weight_lbs,
    quantity: c.quantity,
    notes: c.notes,
  }));

/**
 * A number field that tolerates being empty mid-typing and reports `null`
 * rather than `0` — a blank length means "didn't measure it", which is not
 * the same claim as a fish zero inches long.
 */
function OptionalNumber({
  value,
  onChange,
  placeholder,
  step = '0.1',
}: {
  value: number | null;
  onChange: (next: number | null) => void;
  placeholder: string;
  step?: string;
}) {
  return (
    <Input
      type="number"
      inputMode="decimal"
      min="0"
      step={step}
      placeholder={placeholder}
      value={value ?? ''}
      onChange={(e) => {
        const raw = e.target.value;
        if (raw === '') return onChange(null);
        const parsed = Number(raw);
        onChange(Number.isFinite(parsed) && parsed >= 0 ? parsed : null);
      }}
    />
  );
}

/**
 * The catch log: repeatable rows of species / length / weight / quantity.
 *
 * Species come from the admin-managed list, so a dropdown rather than free
 * text — but every measurement is optional, because plenty of real trips are
 * "we caught a mess of trout" with no tape measure involved.
 */
export function CatchLog({
  catches,
  onChange,
  className,
}: {
  catches: JournalCatchInput[];
  onChange: (next: JournalCatchInput[]) => void;
  className?: string;
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
      </p>

      {catches.length > 0 && (
        <ul className="mb-3 space-y-2">
          {catches.map((c, i) => (
            <li key={i} className="rounded-lg border border-sand bg-white p-3">
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
                <OptionalNumber
                  value={c.length_inches}
                  onChange={(length_inches) => patch(i, { length_inches })}
                  placeholder='Length"'
                />
                <OptionalNumber
                  value={c.weight_lbs}
                  onChange={(weight_lbs) => patch(i, { weight_lbs })}
                  placeholder="Weight lb"
                />
                <Input
                  type="number"
                  inputMode="numeric"
                  min="1"
                  step="1"
                  aria-label="Quantity"
                  placeholder="Qty"
                  value={c.quantity}
                  onChange={(e) => {
                    const parsed = Number.parseInt(e.target.value, 10);
                    patch(i, { quantity: Number.isNaN(parsed) || parsed < 1 ? 1 : parsed });
                  }}
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

/** Read-only summary of a saved catch log, for the feed. */
export function CatchSummary({ catches }: { catches: JournalCatch[] }) {
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

  return (
    <ul className="mt-3 flex flex-wrap gap-1.5">
      {catches.map((c) => (
        <li
          key={c.id}
          title={c.notes ?? undefined}
          className="inline-flex items-center gap-1.5 rounded-full border border-bayou-300 bg-bayou-50 px-2.5 py-0.5 text-xs font-semibold text-bayou-800"
        >
          <Fish size={12} /> {describe(c)}
        </li>
      ))}
    </ul>
  );
}
