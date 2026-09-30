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
The backend requires an SQLite database. The schema is automatically initialized on startup, but you need to set the `DATABASE_URL`.

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

## Deploy
Production runs on `cyrene` at https://event-calendar.miraestudio.id from `~/apps/event-calendar` (a clone of this repo). `compose.yaml` runs backend, scraper and web; web binds `127.0.0.1:3105`, which the host cloudflared tunnel serves. The ingest endpoint is not exposed publicly.

```bash
cd ~/apps/event-calendar
git pull && docker compose up -d --build   # API_KEY lives in .env
```

## Scrapers
Each source lives in its own module under `scraper/src/scrapers/` and is registered in `run_all_scrapers`.
- **Punipun** (`punipun.rs`): parses https://punipun.com/events/ and keeps events starting on or after 2026-10-01.

## System Design Details
- **Batch Ingestion**: Scrapers use a bulk JSON payload mapped directly to relational entities (Series, Events, EventDays, Artists). The Backend API uses a single SQLite Transaction to safely UPSERT this data without locking issues.
- **Memory Footprint**: By utilizing SQLite, Rust, and Next.js ISR, the frontend/API server (`phainon`) footprint is kept well under its 2GB limit.
- **Scraping Tactics**: For optimal speed, the rust scrapers use raw asynchronous HTTP requests, with the flexibility to route through `flaresolverr` to bypass Cloudflare protection if necessary.
