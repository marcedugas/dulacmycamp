-- Access-path indexes and an updated_at trigger.

CREATE INDEX idx_otp_codes_email_active ON otp_codes (email, expires_at DESC)
    WHERE used = false;

CREATE INDEX idx_bookings_user       ON bookings (user_id, created_at DESC);
CREATE INDEX idx_bookings_status     ON bookings (status);
-- Calendar and overlap queries scan by date window.
CREATE INDEX idx_bookings_range      ON bookings (check_in, check_out);

CREATE INDEX idx_blackout_range      ON blackout_dates (start_date, end_date);
CREATE INDEX idx_events_date         ON special_events (event_date);

CREATE INDEX idx_messages_recipient  ON messages (recipient_id, created_at DESC);
CREATE INDEX idx_messages_unread     ON messages (recipient_id) WHERE is_read = false;

CREATE OR REPLACE FUNCTION touch_updated_at() RETURNS trigger AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER users_touch_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TRIGGER bookings_touch_updated_at
    BEFORE UPDATE ON bookings
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
