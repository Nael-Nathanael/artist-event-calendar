-- Scraper that produced each row; a source's rows missing from its latest batch are deleted.
ALTER TABLE events ADD COLUMN source TEXT;
ALTER TABLE event_artists ADD COLUMN source TEXT;

-- What the artist does at the event, e.g. "Guest Cosplayer".
ALTER TABLE event_artists ADD COLUMN role TEXT;
