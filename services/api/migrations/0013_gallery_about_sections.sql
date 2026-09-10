-- Photos belong to one of the three About tabs.
--
-- DEFAULT 'camp' is what carries the existing gallery over: every photo
-- already on the site was uploaded to be the camp's gallery, and the About
-- the Camp tab is where that gallery now lives. Backfilling by default rather
-- than with an UPDATE means no existing row can be missed, and any row
-- inserted by older code in flight lands somewhere sensible too.
ALTER TABLE gallery_photos
    ADD COLUMN about_section text NOT NULL DEFAULT 'camp';

ALTER TABLE gallery_photos
    ADD CONSTRAINT gallery_photos_about_section_valid
    CHECK (about_section IN ('camp', 'dulac', 'last_island'));

-- Photos are read one section at a time, on every landing-page load.
CREATE INDEX idx_gallery_photos_section
    ON gallery_photos (about_section, sort_order, created_at);
