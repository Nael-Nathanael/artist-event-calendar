'use client';
import { useState, useEffect } from 'react';

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
  const [currentDate, setCurrentDate] = useState(new Date());
  const [events, setEvents] = useState<Event[]>([]);
  const [loading, setLoading] = useState(true);

  // Month navigation logic
  const year = currentDate.getFullYear();
  const month = currentDate.getMonth();

  const handlePrevMonth = () => setCurrentDate(new Date(year, month - 1, 1));
  const handleNextMonth = () => setCurrentDate(new Date(year, month + 1, 1));
  
  // Format for header
  const monthName = currentDate.toLocaleString('default', { month: 'long' });

  // Calculate days for the calendar grid
  const firstDayOfMonth = new Date(year, month, 1);
  const startingDayOfWeek = firstDayOfMonth.getDay(); // 0 is Sunday
  
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const daysArray = Array.from({ length: daysInMonth }, (_, i) => i + 1);

  useEffect(() => {
    async function fetchEvents() {
      setLoading(true);
      try {
        const fromStr = `${year}-${String(month + 1).padStart(2, '0')}-01T00:00:00Z`;
        const nextMonth = new Date(year, month + 1, 1);
        const toStr = `${nextMonth.getFullYear()}-${String(nextMonth.getMonth() + 1).padStart(2, '0')}-01T00:00:00Z`;
        
        const backendUrl = process.env.NEXT_PUBLIC_BACKEND_URL || 'http://127.0.0.1:8081';
        const res = await fetch(`${backendUrl}/api/events?from=${fromStr}&to=${toStr}`);
        
        if (res.ok) {
          const data = await res.json();
          setEvents(data);
        } else {
          console.error("Failed to fetch");
        }
      } catch (err) {
        console.error(err);
      } finally {
        setLoading(false);
      }
    }
    fetchEvents();
  }, [year, month]);

  return (
    <main className="min-h-screen p-4 md:p-8 bg-gray-50 text-gray-900">
      <header className="mb-8 flex flex-col md:flex-row justify-between items-start md:items-end gap-4">
        <div>
          <h1 className="text-4xl font-bold text-gray-800 tracking-tight">Artist Event Calendar</h1>
          <p className="text-gray-600 mt-2">Discover upcoming concerts, conventions, and meet & greets.</p>
        </div>
        <div className="flex items-center gap-4 bg-white p-2 rounded-lg shadow-sm border">
          <button onClick={handlePrevMonth} className="px-3 py-1 hover:bg-gray-100 rounded text-gray-700 font-medium">&larr; Prev</button>
          <h2 className="text-xl font-bold text-gray-700 min-w-[140px] text-center">{monthName} {year}</h2>
          <button onClick={handleNextMonth} className="px-3 py-1 hover:bg-gray-100 rounded text-gray-700 font-medium">Next &rarr;</button>
        </div>
      </header>

      <section className="bg-white rounded-xl shadow p-4 md:p-6 overflow-x-auto">
        <div className="min-w-[800px]">
          {/* Calendar Header */}
          <div className="grid grid-cols-7 gap-2 mb-2 text-center font-semibold text-gray-600">
            <div>Sun</div><div>Mon</div><div>Tue</div><div>Wed</div><div>Thu</div><div>Fri</div><div>Sat</div>
          </div>
          
          {/* Calendar Grid */}
          <div className="grid grid-cols-7 gap-2">
            {/* Empty slots for start of month */}
            {Array.from({ length: startingDayOfWeek }).map((_, i) => (
              <div key={`empty-${i}`} className="min-h-32 border rounded bg-gray-50/50"></div>
            ))}
            
            {daysArray.map(day => {
              const dateString = `${year}-${String(month + 1).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
              
              // Find events overlapping with this day
              const dayEvents = events.filter(e => {
                const s = new Date(e.start_date).toLocaleString('en-US', { timeZone: 'Asia/Jakarta' });
                const eEnd = new Date(e.end_date).toLocaleString('en-US', { timeZone: 'Asia/Jakarta' });
                const current = new Date(`${dateString}T12:00:00+07:00`).toLocaleString('en-US', { timeZone: 'Asia/Jakarta' });
                
                // Simplified overlap logic for same day
                return s.split(',')[0] === current.split(',')[0] || eEnd.split(',')[0] === current.split(',')[0];
              });

              return (
                <div key={day} className="min-h-32 border rounded p-2 flex flex-col bg-white hover:bg-gray-50 transition-colors relative">
                  <span className="text-sm font-medium text-gray-500 mb-1">{day}</span>
                  <div className="flex-1 flex flex-col gap-1 overflow-y-auto">
                    {loading && day === 1 ? <span className="text-xs text-gray-400">Loading...</span> : null}
                    {dayEvents.map(event => (
                      <a 
                        key={event.id}
                        href={event.official_url || '#'}
                        target="_blank"
                        rel="noreferrer"
                        className="text-xs bg-blue-100 text-blue-800 p-1 rounded truncate shadow-sm cursor-pointer hover:bg-blue-200"
                        title={`${event.title} - ${event.location_city || ''}`}
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
