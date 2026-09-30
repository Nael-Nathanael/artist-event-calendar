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

const place = (e: Event) =>
  [e.location_name, e.location_city].filter(Boolean).join(", ");

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
  const dialogRef = useRef<HTMLDialogElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
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

  // Keep the URL shareable: the visible month and the open event.
  useEffect(() => {
    if (!currentDate) return;
    const q = new URLSearchParams(window.location.search);
    q.set("year", String(year));
    q.set("month", String(month + 1));
    if (selected) q.set("event", selected.id);
    else if (!pendingEvent) q.delete("event");
    window.history.replaceState(null, "", `?${q}`);
  }, [currentDate, year, month, selected, pendingEvent]);

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
  const cells = weeks * 7;

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
    events.filter(
      (e) => dayKey(e.start_date) <= key && key <= dayKey(e.end_date),
    );

  return (
    <div className="min-h-screen bg-surface text-on-surface expanded:flex expanded:h-dvh expanded:min-h-0 expanded:flex-col">
      <header
        className={`sticky top-0 z-10 flex h-16 shrink-0 items-center gap-2 px-4 transition-colors duration-200 ease-standard ${scrolled ? "bg-surface-container" : "bg-surface"}`}
      >
        <h1 className="title-large hidden truncate medium:block medium:mr-4">
          Artist Event Calendar
        </h1>
        {currentDate && (
          <>
            <button
              type="button"
              onClick={() => setCurrentDate(new Date())}
              className="state-layer inline-flex h-10 shrink-0 items-center rounded-full border border-outline px-6 label-large text-primary"
            >
              Today
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
              {monthLabel}
            </h2>
          </>
        )}
        {loading && (
          <div
            role="progressbar"
            aria-label="Loading events"
            className="linear-progress absolute inset-x-0 bottom-0 h-1 overflow-hidden"
          />
        )}
      </header>

      {currentDate && (
        <main className="mx-auto w-full max-w-7xl px-4 pb-6 expanded:flex expanded:min-h-0 expanded:flex-1 expanded:flex-col expanded:px-6 expanded:pb-4">
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
            className="hidden min-h-0 flex-1 flex-col expanded:flex"
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
              className="grid min-h-0 flex-1 grid-cols-7 gap-px overflow-hidden rounded-lg border border-outline-variant bg-outline-variant"
            >
              {Array.from({ length: cells }, (_, i) => {
                const day = i - leading + 1;
                if (day < 1 || day > daysInMonth) {
                  return (
                    <div
                      key={`pad-${year}-${month}-${String(i)}`}
                      className="bg-surface-container-low"
                    />
                  );
                }
                const key = `${year}-${String(month + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
                const isToday = key === today;
                const dayEvents = eventsOn(key);
                const shown =
                  dayEvents.length > perCell
                    ? dayEvents.slice(0, perCell - 1)
                    : dayEvents;
                return (
                  <div
                    key={key}
                    className="flex min-h-0 flex-col gap-1 overflow-hidden bg-surface p-1"
                  >
                    <span
                      className={`mx-auto flex size-6 shrink-0 items-center justify-center rounded-full label-medium ${isToday ? "border border-primary text-primary" : "text-on-surface-variant"}`}
                      aria-current={isToday ? "date" : undefined}
                    >
                      {day}
                    </span>
                    {shown.map((e) => (
                      <button
                        key={e.id}
                        type="button"
                        onClick={() => setSelected(e)}
                        title={e.title}
                        className="state-layer h-5 w-full shrink-0 truncate rounded-xs bg-secondary-container px-2 text-left label-medium text-on-secondary-container"
                      >
                        {e.title}
                      </button>
                    ))}
                    {shown.length < dayEvents.length && (
                      <button
                        type="button"
                        onClick={() => setOpenDay(key)}
                        className="state-layer h-5 w-full shrink-0 rounded-xs px-2 text-left label-medium text-on-surface-variant"
                      >
                        +{dayEvents.length - shown.length} more
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
          </section>

          {/* Compact and medium windows: agenda list */}
          <section className="expanded:hidden" aria-label={monthLabel}>
            {!loading && !failed && events.length === 0 ? (
              <div className="flex flex-col items-center gap-2 py-16 text-on-surface-variant">
                <Icon name="event_busy" size={48} />
                <p className="body-large">No events this month</p>
              </div>
            ) : (
              <ul className="overflow-hidden rounded-lg bg-surface-container-low">
                {events.map((e) => {
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
            <ul className="-mx-6">
              {eventsOn(openDay).map((e) => (
                <li key={e.id}>
                  <button
                    type="button"
                    onClick={() => setSelected(e)}
                    className="state-layer w-full px-6 py-2 text-left"
                  >
                    <span className="block body-large">{e.title}</span>
                    <span className="block body-medium text-on-surface-variant">
                      {[dateRange(e), place(e)].filter(Boolean).join(" · ")}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
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
