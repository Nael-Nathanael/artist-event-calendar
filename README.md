# Artist Event Calendar

A distributed web application to aggregate and display various events (concerts, festivals, exhibitions, pop-culture conventions, and individual artist events) on a monthly calendar.

## Architecture
The application is designed for a distributed environment to separate lightweight serving from heavy web scraping:
- **Frontend (`/frontend`)**: Next.js App Router application optimized with Tailwind CSS and Incremental Static Regeneration (ISR).
- **Backend API (`/backend`)**: A fast, memory-efficient Rust server using `Axum`, `SQLx`, and **SQLite**.
- **Scraper Worker (`/scraper`)**: A standalone Rust asynchronous worker (`reqwest` + `tokio`) designed to run on a separate server. It periodically scrapes event websites and pushes structured data to the Backend API via batching.

## Requirements
- Rust (`cargo`, `rustc`)
- Bun (`bun`)
- SQLite

## Setup Instructions

### 1. Database & Backend Setup
The backend requires an SQLite database. Migrations in `backend/migrations/` run automatically on startup; you need to set the `DATABASE_URL`. After changing a query or migration, refresh the offline query data with `cargo sqlx prepare`.

```bash
cd backend
export DATABASE_URL="sqlite:events.db"
cargo run
```
The backend will run on `http://127.0.0.1:8081`.

### 2. Scraper Worker Setup
The scraper aggregates event data and pushes it to the backend's internal batch endpoint.

```bash
cd scraper
export BACKEND_URL="http://127.0.0.1:8081"
cargo run
```

### 3. Frontend Setup
The frontend uses `bun` for package management.

```bash
cd frontend
bun install
bun run dev
```
The frontend will run on `http://localhost:3000`. It proxies `/api/events` to `BACKEND_URL` (default `http://127.0.0.1:8081`).

## Search
The search button in the app bar looks across every month. `GET /api/events?q=text` matches the text, case-insensitively, against an event's title, venue, city, organizer and line-up names; `from` and `to` still narrow the dates when given.

## Deploy
Production runs on `cyrene` at https://event-calendar.miraestudio.id from `~/apps/event-calendar` (a clone of this repo). `compose.yaml` runs backend, scraper and web; web binds `127.0.0.1:3105`, which the host cloudflared tunnel serves. The ingest endpoint is not exposed publicly.

```bash
cd ~/apps/event-calendar
git pull && docker compose up -d --build   # API_KEY lives in .env
```

## Scrapers
Each source lives in its own module under `scraper/src/scrapers/` and is registered in the `scrapers` list in `scraper/src/main.rs`.
- **Punipun** (`punipun.rs`): parses https://punipun.com/events/ and keeps events starting on or after 2026-10-01.
- **JIExpo** (`jiexpo.rs`): calls the EventON AJAX endpoint behind https://exhibition.jiexpo.com/event-directory/ for everything from 2026-10-01 up to three years out. Categories come from the WP REST `event_type` of each event.
- **Eventfest** (`eventfest.rs`): the same EventON endpoint behind https://eventfest.id/kalender-event/, from 2026-10-01. Category is the `event_type` term, city the `event_type_2` term. Shared EventON code lives in `eventon.rs`.
- **Eventseye** (`eventseye.rs`): the Indonesia trade-show list at https://www.eventseye.com/fairs/c1_trade-shows_indonesia.html and its numbered pages, from 2026-10-01, category `TradeShow`. Only the list pages are read. Shows dated only by month ("Oct. 2026") are skipped.
- **RuangCosplay** (`ruangcosplay.rs`): reads the list at https://ruangcosplay.com/event, then each event page's JSON-LD, description and source link. Keeps events starting on or after 2026-10-01, category `Cosplay`.
- **Duplicates** (`scraper/src/dedupe.rs`): events from different sources that start the same day merge when their titles are close, when one title contains the whole other at the same venue, or when their venues overlap and the titles are loosely alike. The venue rule is off when a source lists several events at that venue that day, so co-located trade shows stay separate. The earlier source in that list wins and takes the others' missing fields.
- **Artist appearances**: an appearance (e.g. Punipun's "Guest Cosplayer at Itasha Domei") that starts the same day at the same venue as a listed event joins that event's line-up with its role, and the standalone appearance drops out. Appearances with no matching event stay on their own.
- **Removed events**: each event and line-up entry records its source. A batch lists the sources that succeeded; the backend deletes their rows missing from it, which also clears merged duplicates. A source that fails keeps its rows until its next successful run.
- **JMWF** (`jmwf.rs`): Jakarta Mega Wedding Festival from https://www.instagram.com/jmwf.id/. Instagram wants a login for plain HTTP, so the profile loads in headless Chromium through the `flaresolverr` service (`FLARESOLVERR_URL`). Each edition comes from the "📅 dates" and "📍 venue" lines in post captions. Runs monthly.
- Sources run once per hour unless they set a longer `interval` (JMWF: 30 days, counted from its last successful ingest). The scraper loops with a one-hour sleep, not cron, and runs every source immediately on each container start.

## System Design Details
- **Batch Ingestion**: Scrapers use a bulk JSON payload mapped directly to relational entities (Series, Events, EventDays, Artists). The Backend API uses a single SQLite Transaction to safely UPSERT this data without locking issues.
- **Memory Footprint**: By utilizing SQLite, Rust, and Next.js ISR, the frontend/API server (`phainon`) footprint is kept well under its 2GB limit.
- **Scraping Tactics**: For optimal speed, the rust scrapers use raw asynchronous HTTP requests, with the flexibility to route through `flaresolverr` to bypass Cloudflare protection if necessary.
