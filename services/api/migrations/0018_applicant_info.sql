-- Applicant background on booking requests: how to reach them, roughly where
-- they are from, who sent them, and what they are bringing.
--
-- Person-level facts live on the user so a returning guest is prefilled;
-- stay-level facts live on the booking. `users.phone` already exists from
-- 0001 (the profile form's phone field), so it is reused rather than added.
--
-- `phone` and `mailing_address` hold application-level ciphertext (see
-- `pii.rs`), so these two columns are unreadable in a raw database dump.
-- A pre-existing plaintext `phone` is left as it is — no backfill — and is
-- encrypted the next time it is saved.
--
-- Everything is nullable and nothing is backfilled: existing users and
-- bookings keep working with these empty and the UI shows "Not provided".
-- Forward-only.

ALTER TABLE users
    ADD COLUMN home_city       text,
    ADD COLUMN home_state      text,
    ADD COLUMN mailing_address text;   -- optional, free text, encrypted

-- text + CHECK rather than a Postgres ENUM, matching how `role` and
-- `status` are modelled elsewhere in this schema (an enum is painful to
-- change forward-only).
ALTER TABLE bookings
    ADD COLUMN referral           text,
    ADD COLUMN watercraft         text,
    ADD COLUMN watercraft_details text,
    ADD CONSTRAINT bookings_watercraft_valid
        CHECK (watercraft IS NULL OR watercraft IN ('none', 'boat', 'kayak_canoe', 'other'));
