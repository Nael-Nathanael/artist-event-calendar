"use client";

import { useEffect, useRef, useState } from "react";
import { Icon } from "./icons";

type Event = {
  id: string;
  series_id: string | null;
  title: string;
  category: string;
  location_name: string | null;
  location_city: string | null;
  floorplan_image_url: string | null;
  banner_image_url: string | null;
  official_url: string | null;
  start_date: string;
  end_date: string;
  description: string | null;
  organizer: string | null;
  artists: { name: string; role: string | null }[];
};

const TZ = "Asia/Jakarta";
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

// YYYY-MM-DD in WIB, so events land on the day they happen in Indonesia.
const dayKey = (d: Date | string) =>
  new Intl.DateTimeFormat("en-CA", { timeZone: TZ }).format(new Date(d));

const shortDate = new Intl.DateTimeFormat("en-GB", {
  timeZone: TZ,
  weekday: "short",
  day: "numeric",
  month: "short",
});

function dateRange(e: Event) {
  const start = shortDate.format(new Date(e.start_date));
  const end = shortDate.format(new Date(e.end_date));
  return start === end ? start : `${start} – ${end}`;
}

const monthYear = new Intl.DateTimeFormat("en-GB", {
  timeZone: TZ,
  month: "long",
  year: "numeric",
});

// Search results under the month they start in, in the order they arrive.
function byMonth(events: Event[]): [string, Event[]][] {
  const groups = new Map<string, Event[]>();
  for (const e of events) {
    const m = monthYear.format(new Date(e.start_date));
    groups.set(m, [...(groups.get(m) ?? []), e]);
  }
  return [...groups];
}

const clock = new Intl.DateTimeFormat("en-GB", {
  timeZone: TZ,
  hour: "2-digit",
  minute: "2-digit",
});

// Published start/end times; null for all-day events. Multi-day events start
// on the first day and end on the last, so say which is which.
function hours(e: Event) {
  const start = clock.format(new Date(e.start_date));
  const end = clock.format(new Date(e.end_date));
  if (start === "00:00" && end === "23:59") return null;
  return dayKey(e.start_date) === dayKey(e.end_date)
    ? `${start} – ${end}`
    : `Starts ${start} · ends ${end}`;
}

// Scraped categories grouped into the filters people pick from; anything
// unlisted falls under "Other".
const GROUPS = [
  {
    id: "pop",
    label: "Cosplay & pop culture",
    categories: ["Cosplay", "ArtistEvent"],
  },
  {
    id: "music",
    label: "Music",
    categories: ["MusikKonser", "ConcertFestival"],
  },
  {
    id: "shows",
    label: "Festivals & shows",
    categories: [
      "FestivalHiburan",
      "SeniPertunjukan",
      "TheaterMusical",
      "TheaterDanMusical",
    ],
  },
  {
    id: "expo",
    label: "Exhibitions",
    categories: ["Exhibition", "Mice", "BazaarUmkm"],
  },
  { id: "trade", label: "Trade shows", categories: ["TradeShow"] },
  { id: "other", label: "Other", categories: [] },
];
const groupOf = (e: Event) =>
  GROUPS.find((g) => g.categories.includes(e.category))?.id ?? "other";

// Days elapsed in the current month right now in WIB: 3.5 is midday on the 4th.
function nowInMonth() {
  const now = new Date();
  const [h, m] = clock.format(now).split(":").map(Number);
  return Number(dayKey(now).slice(8)) - 1 + (h * 60 + m) / 1440;
}

const place = (e: Event) =>
  [e.location_name, e.location_city].filter(Boolean).join(", ");

// Convention centres the sources spell differently, where most shows share a venue.
const VENUES: [string, RegExp][] = [
  ["JIExpo Kemayoran, Jakarta", /jiexpo|jakarta international expo/i],
  ["ICE BSD, Tangerang", /\bICE\b|indonesian? convention exhibition/i],
  ["NICE PIK 2, Tangerang", /\bNICE\b|nusantara international convention/i],
  ["JCC Senayan, Jakarta", /\bJCC\b|jakarta convention cent/i],
];

// Venue and city without the hall ("JIExpo Kemayoran — Hall B" -> "JIExpo
// Kemayoran, Jakarta"), so a venue's co-located shows land together.
const venueOf = (e: Event) =>
  VENUES.find(([, re]) => re.test(e.location_name ?? ""))?.[0] ??
  [e.location_name?.split(" — ")[0], e.location_city]
    .filter(Boolean)
    .join(", ");

// Events grouped by venue: busiest venue first, unknown venue last.
function byVenue(events: Event[]): [string, Event[]][] {
  const groups = new Map<string, Event[]>();
  for (const e of events) {
    const v = venueOf(e);
    groups.set(v, [...(groups.get(v) ?? []), e]);
  }
  return [...groups].sort(([a, x], [b, y]) =>
    !a !== !b ? (a ? -1 : 1) : y.length - x.length,
  );
}

// An event's days within the shown month (1-based, inclusive) and whether it
// carries on before or after them.
type Span = {
  event: Event;
  from: number;
  to: number;
  before: boolean;
  after: boolean;
};

function monthSpans(events: Event[], year: number, month: number): Span[] {
  const days = new Date(year, month + 1, 0).getDate();
  const prefix = `${year}-${String(month + 1).padStart(2, "0")}-`;
  const dayOf = (key: string) =>
    key < `${prefix}01`
      ? 0
      : key.startsWith(prefix)
        ? Number(key.slice(8))
        : days + 1;
  return events.flatMap((event) => {
    const start = dayOf(dayKey(event.start_date));
    const end = dayOf(dayKey(event.end_date));
    const from = Math.max(1, start);
    const to = Math.min(days, end);
    return from <= to
      ? [{ event, from, to, before: start < from, after: end > to }]
      : [];
  });
}

type Bar = Span & { lane: number };

// A week's pieces of each span, packed into the lowest free lane; among spans
// starting the same day the longest goes first so bars stay straight.
function weekBars(spans: Span[], firstDay: number): Bar[] {
  const lastDay = firstDay + 6;
  const laneEnds: number[] = [];
  return spans
    .flatMap((s) => {
      const from = Math.max(s.from, firstDay);
      const to = Math.min(s.to, lastDay);
      return from <= to
        ? [
            {
              ...s,
              from,
              to,
              before: s.before || from > s.from,
              after: s.after || to < s.to,
            },
          ]
        : [];
    })
    .sort(
      (a, b) =>
        a.from - b.from ||
        b.to - b.from - (a.to - a.from) ||
        a.event.start_date.localeCompare(b.event.start_date),
    )
    .map((s) => {
      let lane = laneEnds.findIndex((end) => end < s.from);
      if (lane === -1) lane = laneEnds.length;
      laneEnds[lane] = s.to;
      return { ...s, lane };
    });
}

// Plain-text description with http(s) and www. links made clickable.
function Linkified({ text }: { text: string }) {
  return text.split(/((?:https?:\/\/|www\.)[^\s]+)/).map((part, i) =>
    /^(https?:\/\/|www\.)/.test(part) ? (
      <a
        // biome-ignore lint/suspicious/noArrayIndexKey: split output is static for a given text
        key={i}
        href={part.startsWith("www.") ? `https://${part}` : part}
        target="_blank"
        rel="noreferrer"
        className="text-primary underline"
      >
        {part}
      </a>
    ) : (
      part
    ),
  );
}

const iconButton =
  "state-layer inline-flex size-10 shrink-0 items-center justify-center rounded-full text-on-surface-variant after:absolute after:-inset-1 after:content-['']";
const textButton =
  "state-layer inline-flex h-10 items-center gap-2 rounded-full px-3 label-large text-primary";

export default function Home() {
  const [currentDate, setCurrentDate] = useState<Date | null>(null);
  const [events, setEvents] = useState<Event[]>([]);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [scrolled, setScrolled] = useState(false);
  const [selected, setSelected] = useState<Event | null>(null);
  const [openDay, setOpenDay] = useState<string | null>(null);
  const [perCell, setPerCell] = useState(3);
  const [view, setView] = useState<"month" | "timeline">("month");
  const [hidden, setHidden] = useState<string[]>([]);
  // Search text; null while the search bar is closed.
  const [query, setQuery] = useState<string | null>(null);
  // Matches across every month; null until a search has answered.
  const [results, setResults] = useState<Event[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [searchFailed, setSearchFailed] = useState(false);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const timelineRef = useRef<HTMLElement>(null);
  // Event id from a shared link, opened once its month's events arrive.
  const [pendingEvent, setPendingEvent] = useState<string | null>(null);

  useEffect(() => {
    const q = new URLSearchParams(window.location.search);
    const y = Number(q.get("year"));
    const m = Number(q.get("month"));
    setCurrentDate(
      Number.isInteger(y) &&
        y >= 1970 &&
        Number.isInteger(m) &&
        m >= 1 &&
        m <= 12
        ? new Date(y, m - 1, 1)
        : new Date(),
    );
    setPendingEvent(q.get("event"));
    if (q.get("view") === "timeline") setView("timeline");
    const hide = q.get("hide")?.split(",") ?? [];
    setHidden(GROUPS.map((g) => g.id).filter((id) => hide.includes(id)));
    setQuery(q.get("q"));
    const onScroll = () => setScrolled(window.scrollY > 0);
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  const year = currentDate?.getFullYear() || 2026;
  const month = currentDate?.getMonth() || 0;

  useEffect(() => {
    if (!currentDate) return;

    const controller = new AbortController();
    const { signal } = controller;

    async function fetchEvents() {
      setLoading(true);
      setFailed(false);
      try {
        const fromStr = `${year}-${String(month + 1).padStart(2, "0")}-01T00:00:00+07:00`;
        const nextMonth = new Date(year, month + 1, 1);
        const toStr = `${nextMonth.getFullYear()}-${String(nextMonth.getMonth() + 1).padStart(2, "0")}-01T00:00:00+07:00`;

        const res = await fetch(
          `/api/events?from=${encodeURIComponent(fromStr)}&to=${encodeURIComponent(toStr)}`,
          { signal },
        );
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        setEvents(await res.json());
      } catch (err) {
        if (err instanceof Error && err.name !== "AbortError") {
          console.error(err);
          setEvents([]);
          setFailed(true);
        }
      } finally {
        if (!signal.aborted) setLoading(false);
      }
    }

    fetchEvents();
    return () => controller.abort();
  }, [year, month, currentDate]);

  useEffect(() => {
    if (loading || !pendingEvent) return;
    const shared = events.find((e) => e.id === pendingEvent);
    setPendingEvent(null);
    if (shared) setSelected(shared);
  }, [events, loading, pendingEvent]);

  // Search every month, once typing pauses.
  useEffect(() => {
    const text = query?.trim();
    setResults(null);
    setSearchFailed(false);
    if (!text) {
      setSearching(false);
      return;
    }
    setSearching(true);
    const controller = new AbortController();
    const timer = setTimeout(async () => {
      try {
        const res = await fetch(`/api/events?q=${encodeURIComponent(text)}`, {
          signal: controller.signal,
        });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        setResults(await res.json());
      } catch (err) {
        if (controller.signal.aborted) return;
        console.error(err);
        setSearchFailed(true);
      }
      setSearching(false);
    }, 250);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [query]);

  // Keep the URL shareable: the view, the visible month, the search and the open event.
  useEffect(() => {
    if (!currentDate) return;
    const q = new URLSearchParams(window.location.search);
    if (view === "timeline") q.set("view", view);
    else q.delete("view");
    if (hidden.length) q.set("hide", hidden.join(","));
    else q.delete("hide");
    if (query) q.set("q", query);
    else q.delete("q");
    q.set("year", String(year));
    q.set("month", String(month + 1));
    if (selected) q.set("event", selected.id);
    else if (!pendingEvent) q.delete("event");
    window.history.replaceState(null, "", `?${q}`);
  }, [currentDate, view, hidden, query, year, month, selected, pendingEvent]);

  // Open the timeline scrolled to today when today is in the shown month.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-run when the timeline appears, changes month, or fills in
  useEffect(() => {
    const el = timelineRef.current;
    const now = el?.querySelector<HTMLElement>("[data-now]");
    if (el && now) el.scrollLeft = now.offsetLeft - el.clientWidth / 3;
  }, [view, year, month, loading]);

  useEffect(() => {
    const d = dialogRef.current;
    if ((selected || openDay) && d && !d.open) d.showModal();
  }, [selected, openDay]);

  const monthLabel = new Date(year, month, 1).toLocaleString("en-GB", {
    month: "long",
    year: "numeric",
  });
  const today = dayKey(new Date());
  const leading = new Date(year, month, 1).getDay();
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const weeks = Math.ceil((leading + daysInMonth) / 7);
  const dayKeyOf = (day: number) =>
    `${year}-${String(month + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
  const shown = events.filter((e) => !hidden.includes(groupOf(e)));
  const spans = monthSpans(shown, year, month);

  // How many 20px event blocks (plus 4px gap) fit under the 24px date in a week row.
  const ready = currentDate !== null;
  useEffect(() => {
    const grid = gridRef.current;
    if (!ready || !grid) return;
    const observer = new ResizeObserver(() => {
      const row = grid.clientHeight / weeks;
      setPerCell(Math.max(1, Math.floor((row - 8 - 24) / 24)));
    });
    observer.observe(grid);
    return () => observer.disconnect();
  }, [ready, weeks]);

  const eventsOn = (key: string) =>
    shown.filter(
      (e) => dayKey(e.start_date) <= key && key <= dayKey(e.end_date),
    );

  return (
    <div className="min-h-screen bg-surface text-on-surface expanded:flex expanded:h-dvh expanded:min-h-0 expanded:flex-col">
      <header
        className={`sticky top-0 z-10 flex h-16 shrink-0 items-center gap-1 px-4 transition-colors medium:gap-2 duration-200 ease-standard ${scrolled ? "bg-surface-container" : "bg-surface"}`}
      >
        <h1 className="title-large hidden shrink-0 expanded:mr-4 expanded:block">
          Artist Event Calendar
        </h1>
        {currentDate && query !== null && (
          <search className="flex h-12 min-w-0 flex-1 items-center rounded-full bg-surface-container-high px-1 text-on-surface-variant">
            <button
              type="button"
              aria-label="Close search"
              onClick={() => setQuery(null)}
              className={iconButton}
            >
              <Icon name="arrow_back" />
            </button>
            <input
              // biome-ignore lint/a11y/noAutofocus: the field appears because the user asked to search
              autoFocus
              type="text"
              enterKeyHint="search"
              value={query}
              onChange={(ev) => setQuery(ev.target.value)}
              onKeyDown={(ev) => {
                if (ev.key === "Escape") setQuery(null);
              }}
              placeholder="Search events, venues, artists"
              aria-label="Search events"
              className="min-w-0 flex-1 bg-transparent px-2 body-large text-on-surface outline-none placeholder:text-on-surface-variant"
            />
            {query && (
              <button
                type="button"
                aria-label="Clear search"
                onClick={() => setQuery("")}
                className={iconButton}
              >
                <Icon name="close" />
              </button>
            )}
          </search>
        )}
        {currentDate && query === null && (
          <>
            <button
              type="button"
              onClick={() => setCurrentDate(new Date())}
              aria-label="Today"
              className="state-layer inline-flex size-10 shrink-0 items-center justify-center rounded-full border border-outline label-large text-primary medium:w-auto medium:px-6"
            >
              <span className="medium:hidden">
                <Icon name="calendar_today" size={20} />
              </span>
              <span className="hidden medium:inline">Today</span>
            </button>
            <button
              type="button"
              aria-label="Previous month"
              onClick={() => setCurrentDate(new Date(year, month - 1, 1))}
              className={iconButton}
            >
              <Icon name="chevron_left" />
            </button>
            <button
              type="button"
              aria-label="Next month"
              onClick={() => setCurrentDate(new Date(year, month + 1, 1))}
              className={iconButton}
            >
              <Icon name="chevron_right" />
            </button>
            <h2 className="title-large truncate" aria-live="polite">
              <span className="medium:hidden">
                {new Date(year, month, 1).toLocaleString("en-GB", {
                  month: "short",
                  year: "numeric",
                })}
              </span>
              <span className="hidden medium:inline">{monthLabel}</span>
            </h2>
            <button
              type="button"
              aria-label="Search events"
              onClick={() => setQuery("")}
              className={`${iconButton} ml-auto`}
            >
              <Icon name="search" />
            </button>
            <div className="flex h-10 shrink-0 overflow-hidden rounded-full border border-outline">
              {(
                [
                  ["month", "Month", "calendar_view_month"],
                  ["timeline", "Timeline", "view_timeline"],
                ] as const
              ).map(([v, label, icon], i) => (
                <button
                  key={v}
                  type="button"
                  aria-pressed={view === v}
                  aria-label={`${label} view`}
                  onClick={() => setView(v)}
                  className={`state-layer inline-flex items-center gap-2 px-2.5 label-large expanded:px-4 ${i > 0 ? "border-l border-outline" : ""} ${view === v ? "bg-secondary-container text-on-secondary-container" : "text-on-surface"}`}
                >
                  <Icon name={icon} size={18} />
                  <span className="hidden expanded:inline">{label}</span>
                </button>
              ))}
            </div>
          </>
        )}
        {(query === null ? loading : searching) && (
          <div
            role="progressbar"
            aria-label="Loading events"
            className="linear-progress absolute inset-x-0 bottom-0 h-1 overflow-hidden"
          />
        )}
      </header>

      {query !== null && (
        <main className="mx-auto w-full max-w-3xl px-4 pb-6 expanded:min-h-0 expanded:flex-1 expanded:overflow-y-auto">
          {searchFailed ? (
            <p className="rounded-md bg-surface-container-high px-4 py-3 body-medium">
              Couldn&apos;t search events.
            </p>
          ) : !results?.length ? (
            <div className="flex flex-col items-center gap-2 py-16 text-center text-on-surface-variant">
              <Icon name={results ? "event_busy" : "search"} size={48} />
              <p className="body-large">
                {results
                  ? `No events match “${query.trim()}”`
                  : "Search by event, venue, city, organizer or artist"}
              </p>
            </div>
          ) : (
            <>
              <p
                className="pb-2 label-large text-on-surface-variant"
                aria-live="polite"
              >
                {results.length} {results.length === 1 ? "event" : "events"}
              </p>
              {byMonth(results).map(([label, list]) => (
                <section key={label} aria-label={label}>
                  <h3 className="px-4 pb-1 pt-3 title-small text-on-surface-variant">
                    {label}
                  </h3>
                  <ul className="overflow-hidden rounded-lg bg-surface-container-low">
                    {list.map((e) => (
                      <li key={e.id}>
                        <button
                          type="button"
                          onClick={() => setSelected(e)}
                          className="state-layer w-full px-4 py-3 text-left"
                        >
                          <span className="block body-large line-clamp-2">
                            {e.title}
                          </span>
                          <span className="block body-medium text-on-surface-variant">
                            {[
                              dateRange(e),
                              place(e),
                              ...e.artists.map((a) => a.name),
                            ]
                              .filter(Boolean)
                              .join(" · ")}
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
                </section>
              ))}
            </>
          )}
        </main>
      )}

      {currentDate && (
        <main
          hidden={query !== null}
          className="mx-auto w-full max-w-7xl px-4 pb-6 expanded:flex expanded:min-h-0 expanded:flex-1 expanded:flex-col expanded:px-6 expanded:pb-4"
        >
          {/* Filter chips: every group starts on; tapping one hides its events. */}
          <fieldset className="-mx-4 flex min-w-0 shrink-0 gap-2 overflow-x-auto px-4 pb-3 [scrollbar-width:none] expanded:-mx-6 expanded:px-6">
            <legend className="sr-only">Filter by category</legend>
            {GROUPS.map((g) => {
              const on = !hidden.includes(g.id);
              const count = events.filter((e) => groupOf(e) === g.id).length;
              if (on && count === 0) return null;
              return (
                <button
                  key={g.id}
                  type="button"
                  aria-pressed={on}
                  onClick={() =>
                    setHidden((h) =>
                      on ? [...h, g.id] : h.filter((id) => id !== g.id),
                    )
                  }
                  className={`state-layer inline-flex h-8 shrink-0 items-center gap-2 rounded-sm label-large ${on ? "bg-secondary-container pl-2 pr-4 text-on-secondary-container" : "border border-outline px-4 text-on-surface-variant"}`}
                >
                  {on && <Icon name="check" size={18} />}
                  {g.label}
                  <span className="text-on-surface-variant">{count}</span>
                </button>
              );
            })}
          </fieldset>

          {failed && (
            <div className="mb-4 flex items-center gap-2 rounded-md bg-surface-container-high py-2 pl-4 pr-2 body-medium">
              <span className="flex-1">Couldn&apos;t load events.</span>
              <button
                type="button"
                onClick={() => setCurrentDate(new Date(year, month, 1))}
                className={textButton}
              >
                Retry
              </button>
            </div>
          )}

          {/* Expanded window: month grid */}
          <section
            className={`hidden min-h-0 flex-1 flex-col ${view === "month" ? "expanded:flex" : ""}`}
            aria-label={monthLabel}
          >
            <div className="grid grid-cols-7">
              {WEEKDAYS.map((d) => (
                <div
                  key={d}
                  className="py-1 text-center label-medium text-on-surface-variant"
                >
                  {d}
                </div>
              ))}
            </div>
            <div
              ref={gridRef}
              style={{ gridTemplateRows: `repeat(${weeks}, minmax(0, 1fr))` }}
              className="grid min-h-0 flex-1 gap-px overflow-hidden rounded-lg border border-outline-variant bg-outline-variant"
            >
              {Array.from({ length: weeks }, (_, w) => {
                const firstDay = w * 7 - leading + 1;
                const bars = weekBars(spans, firstDay);
                const covering = (col: number) =>
                  bars.filter(
                    (b) => b.from - firstDay <= col && col <= b.to - firstDay,
                  );
                const busiest = Math.max(
                  0,
                  ...WEEKDAYS.map((_, c) => covering(c).length),
                );
                // Leave the last lane for "+N more" when a day overflows.
                const lanes = busiest > perCell ? perCell - 1 : perCell;
                return (
                  <div
                    key={`${year}-${month}-${firstDay}`}
                    className="relative grid min-h-0 grid-cols-7 gap-px"
                  >
                    {WEEKDAYS.map((_, c) => {
                      const day = firstDay + c;
                      if (day < 1 || day > daysInMonth) {
                        return (
                          <div
                            key={`pad-${String(day)}`}
                            className="bg-surface-container-low"
                          />
                        );
                      }
                      const isToday = dayKeyOf(day) === today;
                      return (
                        <div key={day} className="bg-surface p-1">
                          <span
                            className={`mx-auto flex size-6 items-center justify-center rounded-full label-medium ${isToday ? "border border-primary text-primary" : "text-on-surface-variant"}`}
                            aria-current={isToday ? "date" : undefined}
                          >
                            {day}
                          </span>
                        </div>
                      );
                    })}
                    <div className="pointer-events-none absolute inset-0 grid auto-rows-[20px] grid-cols-7 content-start gap-x-px gap-y-1 pt-8">
                      {bars
                        .filter((b) => b.lane < lanes)
                        .map((b) => (
                          <button
                            key={b.event.id}
                            type="button"
                            onClick={() => setSelected(b.event)}
                            title={b.event.title}
                            style={{
                              gridColumn: `${b.from - firstDay + 1} / span ${b.to - b.from + 1}`,
                              gridRow: b.lane + 1,
                            }}
                            className={`state-layer pointer-events-auto min-w-0 truncate bg-secondary-container px-2 text-left label-medium text-on-secondary-container ${b.before ? "" : "ml-1 rounded-l-xs"} ${b.after ? "" : "mr-1 rounded-r-xs"}`}
                          >
                            {b.event.title}
                          </button>
                        ))}
                      {WEEKDAYS.map((_, c) => {
                        const hidden = covering(c).filter(
                          (b) => b.lane >= lanes,
                        ).length;
                        const day = firstDay + c;
                        return hidden > 0 ? (
                          <button
                            key={`more-${String(day)}`}
                            type="button"
                            onClick={() => setOpenDay(dayKeyOf(day))}
                            style={{ gridColumn: c + 1, gridRow: lanes + 1 }}
                            className="state-layer pointer-events-auto mx-1 min-w-0 truncate rounded-xs px-2 text-left label-medium text-on-surface-variant"
                          >
                            +{hidden} more
                          </button>
                        ) : null;
                      })}
                    </div>
                  </div>
                );
              })}
            </div>
          </section>

          {/* Compact and medium windows: agenda list */}
          <section
            className={view === "month" ? "expanded:hidden" : "hidden"}
            aria-label={monthLabel}
          >
            {!loading && !failed && shown.length === 0 ? (
              <div className="flex flex-col items-center gap-2 py-16 text-on-surface-variant">
                <Icon name="event_busy" size={48} />
                <p className="body-large">No events this month</p>
              </div>
            ) : (
              <ul className="overflow-hidden rounded-lg bg-surface-container-low">
                {shown.map((e) => {
                  const start = new Date(e.start_date);
                  return (
                    <li key={e.id}>
                      <button
                        type="button"
                        onClick={() => setSelected(e)}
                        className="state-layer flex min-h-[72px] w-full items-start gap-4 px-4 py-3 text-left"
                      >
                        <span className="flex w-10 shrink-0 flex-col items-center">
                          <span className="label-medium text-on-surface-variant">
                            {start.toLocaleString("en-GB", {
                              timeZone: TZ,
                              weekday: "short",
                            })}
                          </span>
                          <span className="title-large">
                            {start.toLocaleString("en-GB", {
                              timeZone: TZ,
                              day: "numeric",
                            })}
                          </span>
                        </span>
                        <span className="min-w-0">
                          <span className="block body-large line-clamp-2">
                            {e.title}
                          </span>
                          <span className="block body-medium text-on-surface-variant">
                            {[dateRange(e), place(e)]
                              .filter(Boolean)
                              .join(" · ")}
                          </span>
                        </span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </section>

          {/* Timeline: one row per event across the month's days */}
          {view === "timeline" && (
            <section
              ref={timelineRef}
              aria-label={`${monthLabel} timeline`}
              className="max-h-[calc(100dvh-5.5rem)] overflow-auto rounded-lg border border-outline-variant expanded:max-h-none expanded:min-h-0 expanded:flex-1"
            >
              <div
                className="relative"
                style={{ minWidth: `${daysInMonth * 36}px` }}
              >
                <div
                  className="sticky top-0 z-10 grid bg-surface-container-low"
                  style={{ gridTemplateColumns: `repeat(${daysInMonth}, 1fr)` }}
                >
                  {Array.from({ length: daysInMonth }, (_, i) => {
                    const day = i + 1;
                    const isToday = dayKeyOf(day) === today;
                    return (
                      <div
                        key={day}
                        className="flex flex-col items-center py-1 label-medium text-on-surface-variant"
                      >
                        <span>
                          {WEEKDAYS[new Date(year, month, day).getDay()][0]}
                        </span>
                        <span
                          className={`flex size-6 items-center justify-center rounded-full ${isToday ? "bg-primary text-on-primary" : ""}`}
                          aria-current={isToday ? "date" : undefined}
                        >
                          {day}
                        </span>
                      </div>
                    );
                  })}
                </div>
                <div className="relative">
                  <div
                    aria-hidden="true"
                    className="absolute inset-0 grid"
                    style={{
                      gridTemplateColumns: `repeat(${daysInMonth}, 1fr)`,
                    }}
                  >
                    {Array.from({ length: daysInMonth }, (_, i) => {
                      const weekday = new Date(year, month, i + 1).getDay();
                      return (
                        <div
                          key={String(i)}
                          className={`border-l border-outline-variant ${weekday === 0 || weekday === 6 ? "bg-surface-container-low" : ""}`}
                        />
                      );
                    })}
                  </div>
                  {today.startsWith(dayKeyOf(1).slice(0, 8)) && (
                    <div
                      data-now
                      aria-hidden="true"
                      className="absolute inset-y-0 z-[1] w-0.5 bg-primary"
                      style={{ left: `${(nowInMonth() / daysInMonth) * 100}%` }}
                    />
                  )}
                  {!loading && spans.length === 0 ? (
                    <p className="relative py-16 text-center body-large text-on-surface-variant">
                      No events this month
                    </p>
                  ) : (
                    <ul className="relative py-2">
                      {[...spans]
                        .sort(
                          (a, b) =>
                            a.from - b.from ||
                            a.to - b.to ||
                            a.event.title.localeCompare(b.event.title),
                        )
                        .map((s) => {
                          const left = ((s.from - 1) / daysInMonth) * 100;
                          const width =
                            ((s.to - s.from + 1) / daysInMonth) * 100;
                          return (
                            <li key={s.event.id}>
                              <button
                                type="button"
                                onClick={() => setSelected(s.event)}
                                className="state-layer relative block h-10 w-full text-left"
                              >
                                <span
                                  className={`absolute inset-y-1.5 bg-secondary-container ${s.before ? "" : "rounded-l-sm"} ${s.after ? "" : "rounded-r-sm"}`}
                                  style={{
                                    left: `${left}%`,
                                    width: `${width}%`,
                                  }}
                                />
                                {/* The title starts on the bar and may run past a short one. */}
                                <span
                                  className="absolute top-1/2 -translate-y-1/2 truncate label-large text-on-secondary-container"
                                  style={{
                                    left: `calc(${left}% + 8px)`,
                                    maxWidth: `calc(${100 - left}% - 16px)`,
                                  }}
                                >
                                  {s.event.title}
                                </span>
                              </button>
                            </li>
                          );
                        })}
                    </ul>
                  )}
                </div>
              </div>
            </section>
          )}
        </main>
      )}

      {/* biome-ignore lint/a11y/useKeyWithClickEvents: backdrop click is pointer-only; Escape closes the native dialog. */}
      <dialog
        ref={dialogRef}
        onClose={() => {
          setSelected(null);
          setOpenDay(null);
        }}
        onClick={(ev) => {
          if (ev.target === dialogRef.current) dialogRef.current.close();
        }}
        aria-labelledby="event-title"
        className="m-auto w-[calc(100vw-48px)] min-w-[280px] max-w-[560px] rounded-xl bg-surface-container-high p-0 text-on-surface"
      >
        {selected?.banner_image_url && (
          // biome-ignore lint/performance/noImgElement: remote images from many scraped hosts
          <img
            src={selected.banner_image_url}
            alt=""
            className="max-h-72 w-full bg-surface-container object-cover"
          />
        )}
        {selected && (
          <div className="p-6">
            <h2 id="event-title" className="headline-small mb-4">
              {selected.title}
            </h2>
            <div className="flex flex-col gap-3 body-medium text-on-surface-variant">
              <p className="flex items-center gap-3">
                <Icon name="calendar_today" size={20} />
                {dateRange(selected)}
              </p>
              {hours(selected) && (
                <p className="flex items-center gap-3">
                  <Icon name="schedule" size={20} />
                  {hours(selected)}
                </p>
              )}
              {place(selected) && (
                <p className="flex items-center gap-3">
                  <Icon name="location_on" size={20} />
                  {place(selected)}
                </p>
              )}
              {selected.organizer && (
                <p className="flex items-center gap-3">
                  <Icon name="groups" size={20} />
                  {selected.organizer}
                </p>
              )}
              {selected.artists.map((a) => (
                <p key={a.name} className="flex items-start gap-3">
                  <Icon name="person" size={20} />
                  <span>
                    <span className="text-on-surface">{a.name}</span>
                    {a.role && ` · ${a.role}`}
                  </span>
                </p>
              ))}
            </div>
            {selected.description && (
              <p className="mt-4 whitespace-pre-line break-words body-medium text-on-surface">
                <Linkified text={selected.description} />
              </p>
            )}
            <div className="mt-6 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => dialogRef.current?.close()}
                className={textButton}
              >
                Close
              </button>
              {selected.official_url && (
                <a
                  href={selected.official_url}
                  target="_blank"
                  rel="noreferrer"
                  className={textButton}
                >
                  Official site
                  <Icon name="open_in_new" size={18} />
                </a>
              )}
            </div>
          </div>
        )}
        {!selected && openDay && (
          <div className="p-6">
            <h2 id="event-title" className="headline-small mb-4">
              {shortDate.format(new Date(`${openDay}T12:00:00+07:00`))}
            </h2>
            <div className="-mx-6">
              {byVenue(eventsOn(openDay)).map(([venue, list]) => (
                <section key={venue} aria-label={venue || "Venue not listed"}>
                  <h3 className="flex items-center gap-2 px-6 pb-1 pt-3 title-small text-on-surface-variant">
                    <Icon name="location_on" size={18} />
                    <span className="min-w-0 flex-1 truncate">
                      {venue || "Venue not listed"}
                    </span>
                    <span>{list.length}</span>
                  </h3>
                  <ul>
                    {list.map((e) => (
                      <li key={e.id}>
                        <button
                          type="button"
                          onClick={() => setSelected(e)}
                          className="state-layer w-full py-2 pl-[52px] pr-6 text-left"
                        >
                          <span className="block body-large">{e.title}</span>
                          <span className="block body-medium text-on-surface-variant">
                            {[dateRange(e), e.location_name?.split(" — ")[1]]
                              .filter(Boolean)
                              .join(" · ")}
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
                </section>
              ))}
            </div>
            <div className="mt-4 flex justify-end">
              <button
                type="button"
                onClick={() => dialogRef.current?.close()}
                className={textButton}
              >
                Close
              </button>
            </div>
          </div>
        )}
      </dialog>
    </div>
  );
}
