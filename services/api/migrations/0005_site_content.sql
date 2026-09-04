-- Admin-editable site content: hero/about text, house rules, amenities, and
-- a photo gallery, replacing the hardcoded RULES/AMENITIES arrays and
-- placeholder images in Landing.tsx. No admin data entry needed to move the
-- landing page's copy or photos — it's all rows now.

-- Singleton row for free-text site content. Enforced as exactly one row by
-- fixing its id rather than trusting application logic alone: nothing but
-- this migration is allowed to INSERT here (see users.rs-style guard in the
-- Rust handlers, which only ever UPDATEs).
CREATE TABLE site_settings (
    id                uuid PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001',
    hero_title        text NOT NULL DEFAULT 'Dulac My Camp',
    hero_subtitle     text NOT NULL DEFAULT 'A fishing camp in the heart of Dulac, Louisiana',
    about_text        text NOT NULL DEFAULT '',
    hero_image_url    text,
    -- External link to a shared Google Photos album (or similar) where
    -- guests can add their own trip photos. This is NOT an API integration —
    -- Google deprecated third-party shared album management via API in
    -- March 2025. It's a plain link the admin sets by hand, pointing at an
    -- album they created and shared themselves in the Google Photos app
    -- with "let anyone with the link add photos" turned on.
    guest_photos_url  text,
    updated_at        timestamptz NOT NULL DEFAULT now()
);

INSERT INTO site_settings (id, hero_title, hero_subtitle)
VALUES ('00000000-0000-0000-0000-000000000001', 'Dulac My Camp', 'A fishing camp in the heart of Dulac, Louisiana');

-- Rules list.
CREATE TABLE rules_items (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    text        text NOT NULL,
    sort_order  integer NOT NULL DEFAULT 0,
    created_at  timestamptz NOT NULL DEFAULT now()
);

-- Amenities list.
CREATE TABLE amenities_items (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    label       text NOT NULL,
    -- Optional lucide-react icon name, e.g. "Wifi", "Anchor" — free text;
    -- the frontend falls back to a generic icon if blank or unrecognized.
    icon        text,
    sort_order  integer NOT NULL DEFAULT 0,
    created_at  timestamptz NOT NULL DEFAULT now()
);

-- Gallery photos.
CREATE TABLE gallery_photos (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    url         text NOT NULL,
    caption     text,
    sort_order  integer NOT NULL DEFAULT 0,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_rules_items_sort ON rules_items (sort_order);
CREATE INDEX idx_amenities_items_sort ON amenities_items (sort_order);
CREATE INDEX idx_gallery_photos_sort ON gallery_photos (sort_order);
