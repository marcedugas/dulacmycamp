-- Driving directions, and the split of the single "about" blurb into three
-- separate stories.

-- Free text, used verbatim as the destination in a Google Maps directions
-- URL. Google accepts a street address and a bare "lat,lng" pair
-- interchangeably there, so this deliberately does not validate or parse:
-- some camps have a clean mailing address and some only have coordinates,
-- and both are correct answers to "where is it".
ALTER TABLE site_settings ADD COLUMN camp_address text;

-- One blurb became three. A RENAME rather than a new column plus a copy:
-- whatever is written here today *is* the camp description, so it carries
-- over by construction and there is no window in which it could be missed.
ALTER TABLE site_settings RENAME COLUMN about_text TO about_camp_text;

ALTER TABLE site_settings ADD COLUMN about_dulac_text  text NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN last_island_text  text NOT NULL DEFAULT '';
