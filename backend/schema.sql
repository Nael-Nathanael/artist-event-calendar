-- Represents a grouping of events (e.g., "The Eras Tour")
CREATE TABLE IF NOT EXISTS event_series (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT
);

-- Represents the actual event (e.g., "Pestapora 2024")
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY NOT NULL,
    series_id TEXT REFERENCES event_series(id),
    title TEXT NOT NULL, 
    category TEXT NOT NULL,
    location_name TEXT,
    location_city TEXT,
    floorplan_image_url TEXT,
    banner_image_url TEXT,
    official_url TEXT,
    start_date TEXT NOT NULL,
    end_date TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

-- Represents a specific day in a multi-day event (e.g., "Day 1")
CREATE TABLE IF NOT EXISTS event_days (
    id TEXT PRIMARY KEY NOT NULL,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    name TEXT NOT NULL, 
    date TEXT NOT NULL,
    rundown_json TEXT
);

CREATE TABLE IF NOT EXISTS artists (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    profile_image_url TEXT
);

-- Many-to-Many mapping for Artists at an Event
CREATE TABLE IF NOT EXISTS event_artists (
    event_id TEXT REFERENCES events(id) ON DELETE CASCADE,
    artist_id TEXT REFERENCES artists(id) ON DELETE CASCADE,
    PRIMARY KEY (event_id, artist_id)
);
