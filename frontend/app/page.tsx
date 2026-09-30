"use client";

import { useEffect, useState } from "react";

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
};

export default function Home() {
  const [mounted, setMounted] = useState(false);
  const [currentDate, setCurrentDate] = useState<Date | null>(null);
  const [events, setEvents] = useState<Event[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setCurrentDate(new Date());
    setMounted(true);
  }, []);

  const year = currentDate?.getFullYear() || 2026;
  const month = currentDate?.getMonth() || 0;

  const handlePrevMonth = () => {
    if (currentDate) setCurrentDate(new Date(year, month - 1, 1));
  };

  const handleNextMonth = () => {
    if (currentDate) setCurrentDate(new Date(year, month + 1, 1));
  };

  const monthName =
    currentDate?.toLocaleString("default", { month: "long" }) || "";
  const firstDayOfMonth = new Date(year, month, 1);
  const startingDayOfWeek = firstDayOfMonth.getDay();

  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const daysArray = Array.from({ length: daysInMonth }, (_, i) => i + 1);

  useEffect(() => {
    if (!currentDate) return;

    const controller = new AbortController();
    const { signal } = controller;

    async function fetchEvents() {
      setLoading(true);
      try {
        const fromStr = `${year}-${String(month + 1).padStart(2, "0")}-01T00:00:00+07:00`;
        const nextMonth = new Date(year, month + 1, 1);
        const toStr = `${nextMonth.getFullYear()}-${String(nextMonth.getMonth() + 1).padStart(2, "0")}-01T00:00:00+07:00`;

        const fromEncoded = encodeURIComponent(fromStr);
        const toEncoded = encodeURIComponent(toStr);

        const backendUrl =
          process.env.NEXT_PUBLIC_BACKEND_URL || "http://127.0.0.1:8081";
        const res = await fetch(
          `${backendUrl}/api/events?from=${fromEncoded}&to=${toEncoded}`,
          { signal },
        );

        if (res.ok) {
          const data = await res.json();
          setEvents(data);
        } else {
          console.error("Failed to fetch");
        }
      } catch (err) {
        if (err instanceof Error && err.name !== "AbortError") {
          console.error(err);
        }
      } finally {
        if (!signal.aborted) {
          setLoading(false);
        }
      }
    }

    fetchEvents();

    return () => {
      controller.abort();
    };
  }, [year, month, currentDate]);

  if (!mounted) {
    return (
      <div className="min-h-screen p-8 bg-gray-50 flex items-center justify-center">
        Loading Calendar...
      </div>
    );
  }

  return (
    <main className="min-h-screen p-4 md:p-8 bg-gray-50 text-gray-900">
      <header className="mb-8 flex flex-col md:flex-row justify-between items-start md:items-end gap-4">
        <div>
          <h1 className="text-4xl font-bold text-gray-800 tracking-tight">
            Artist Event Calendar
          </h1>
          <p className="text-gray-600 mt-2">
            Discover upcoming concerts, conventions, and meet & greets.
          </p>
        </div>
        <div className="flex items-center gap-4 bg-white p-2 rounded-lg shadow-sm border">
          <button
            type="button"
            onClick={handlePrevMonth}
            className="px-3 py-1 hover:bg-gray-100 rounded text-gray-700 font-medium"
          >
            &larr; Prev
          </button>
          <h2 className="text-xl font-bold text-gray-700 min-w-[140px] text-center">
            {monthName} {year}
          </h2>
          <button
            type="button"
            onClick={handleNextMonth}
            className="px-3 py-1 hover:bg-gray-100 rounded text-gray-700 font-medium"
          >
            Next &rarr;
          </button>
        </div>
      </header>

      <section className="bg-white rounded-xl shadow p-4 md:p-6 overflow-x-auto">
        <div className="min-w-[800px]">
          <div className="grid grid-cols-7 gap-2 mb-2 text-center font-semibold text-gray-600">
            <div>Sun</div>
            <div>Mon</div>
            <div>Tue</div>
            <div>Wed</div>
            <div>Thu</div>
            <div>Fri</div>
            <div>Sat</div>
          </div>

          <div className="grid grid-cols-7 gap-2">
            {Array.from({ length: startingDayOfWeek }).map((_, i) => (
              <div
                // using a stable key alternative to just index
                key={`empty-slot-${year}-${month}-${String(i)}`}
                className="min-h-32 border rounded bg-gray-50/50"
              />
            ))}

            {daysArray.map((day) => {
              const dateString = `${year}-${String(month + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
              const currentDayWIB = new Date(
                `${dateString}T12:00:00+07:00`,
              ).getTime();

              const dayEvents = events.filter((e) => {
                const sDate = new Date(e.start_date);
                const eDate = new Date(e.end_date);

                // Truncate to midnight in WIB for accurate day overlap matching
                const getWibMidnightTime = (d: Date) => {
                  const parts = new Intl.DateTimeFormat("en-CA", {
                    timeZone: "Asia/Jakarta",
                  }).format(d);
                  return new Date(`${parts}T12:00:00+07:00`).getTime();
                };

                const sTime = getWibMidnightTime(sDate);
                const eTime = getWibMidnightTime(eDate);

                return currentDayWIB >= sTime && currentDayWIB <= eTime;
              });

              return (
                <div
                  key={day}
                  className="min-h-32 border rounded p-2 flex flex-col bg-white hover:bg-gray-50 transition-colors relative"
                >
                  <span className="text-sm font-medium text-gray-500 mb-1">
                    {day}
                  </span>
                  <div className="flex-1 flex flex-col gap-1 overflow-y-auto">
                    {loading && day === 1 ? (
                      <span className="text-xs text-gray-400">Loading...</span>
                    ) : null}
                    {dayEvents.map((event) => (
                      <a
                        key={event.id}
                        href={event.official_url || "#"}
                        target="_blank"
                        rel="noreferrer"
                        className="text-xs bg-blue-100 text-blue-800 p-1 rounded truncate shadow-sm cursor-pointer hover:bg-blue-200"
                        title={`${event.title} - ${event.location_city || ""}`}
                      >
                        {event.title}
                      </a>
                    ))}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </section>
    </main>
  );
}
