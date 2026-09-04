-- Seed the camp owner as an admin so his first OTP login lands him straight
-- in the admin panel, rather than self-registering as a guest and needing a
-- second person to promote him.
--
-- DO UPDATE rather than DO NOTHING so this also covers the case where he has
-- already signed in once and exists as a guest. Migrations run exactly once
-- against a given database, so this cannot silently re-promote him later if
-- the role is deliberately changed.
INSERT INTO users (email, full_name, role)
VALUES ('jldugas@eatel.net', 'Jean Dugas', 'admin')
ON CONFLICT (email) DO UPDATE
    SET role = 'admin',
        full_name = COALESCE(users.full_name, EXCLUDED.full_name);
