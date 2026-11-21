import { Field, Input, Textarea } from './ui';

export interface ContactValues {
  phone: string;
  home_city: string;
  home_state: string;
  mailing_address: string;
}

export interface ContactErrors {
  phone?: string | null;
  home_city?: string | null;
  home_state?: string | null;
}

/** The four person-level fields, shared by the booking form and the profile. */
export function ContactFields({
  values,
  onChange,
  errors = {},
  required = false,
}: {
  values: ContactValues;
  onChange: (next: ContactValues) => void;
  errors?: ContactErrors;
  required?: boolean;
}) {
  const set = (key: keyof ContactValues) => (e: { target: { value: string } }) =>
    onChange({ ...values, [key]: e.target.value });
  const err = (m?: string | null) => m && <p className="mt-1 text-sm text-clay">{m}</p>;
  const star = required ? '' : ' (optional)';

  return (
    <div className="space-y-4">
      <Field label={`Phone number${star}`}>
        <Input
          type="tel"
          inputMode="tel"
          autoComplete="tel"
          value={values.phone}
          onChange={set('phone')}
          placeholder="(985) 555-0134"
        />
        {err(errors.phone)}
      </Field>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label={`City${star}`}>
          <Input
            autoComplete="address-level2"
            maxLength={80}
            value={values.home_city}
            onChange={set('home_city')}
          />
          {err(errors.home_city)}
        </Field>
        <Field label={`State${star}`}>
          <Input
            autoComplete="address-level1"
            maxLength={40}
            value={values.home_state}
            onChange={set('home_state')}
          />
          {err(errors.home_state)}
        </Field>
      </div>
      <Field label="Mailing address (optional)">
        <Textarea
          rows={2}
          autoComplete="street-address"
          maxLength={300}
          value={values.mailing_address}
          onChange={set('mailing_address')}
        />
      </Field>
    </div>
  );
}

export const contactFromUser = (u: {
  phone: string | null;
  home_city: string | null;
  home_state: string | null;
  mailing_address: string | null;
} | null): ContactValues => ({
  phone: u?.phone ?? '',
  home_city: u?.home_city ?? '',
  home_state: u?.home_state ?? '',
  mailing_address: u?.mailing_address ?? '',
});
