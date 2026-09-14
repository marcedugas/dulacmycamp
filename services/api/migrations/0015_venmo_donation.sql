-- Venmo donation prompt on the checkout page.

-- The handle is admin-editable rather than hardcoded, same reasoning as
-- `camp_address` in 0012: it may need to change and shouldn't require a code
-- deploy to do so. `venmo_enabled` is a separate flag rather than "blank
-- handle means off" so the admin can pause the prompt without losing the
-- handle they'd already set.
ALTER TABLE site_settings ADD COLUMN venmo_handle text;
ALTER TABLE site_settings ADD COLUMN venmo_enabled boolean NOT NULL DEFAULT false;

-- Wanted live immediately with this handle, not left blank pending a
-- separate admin step post-deploy.
UPDATE site_settings
SET venmo_handle = 'JeanL-Dugas', venmo_enabled = true
WHERE id = '00000000-0000-0000-0000-000000000001';
