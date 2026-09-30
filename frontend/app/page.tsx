import { Suspense } from 'react';

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

async function getEvents(): Promise<Event[]> {
  try {
    const backendUrl = process.env.NEXT_PUBLIC_BACKEND_URL || 'http://127.0.0.1:8081';
    const res = await fetch(`${backendUrl}/api/events`, {
      next: { revalidate: 60 }, // ISR
    });
    if (!res.ok) {
      throw new Error('Failed to fetch data');
    }
    return res.json();
  } catch (err) {
    console.error(err);
    return [];
  }
}

export default async function Home() {
  const events = await getEvents();

  // Simple hardcoded month generation (e.g. October 2026 for demonstration)
  // In a real app, use query params to change months.
  const daysInMonth = Array.from({ length: 31 }, (_, i) => i + 1);

  return (
    <main className="min-h-screen p-8 bg-gray-50 text-gray-900">
      <header className="mb-8 flex justify-between items-end">
        <div>
          <h1 className="text-4xl font-bold text-gray-800 tracking-tight">Artist Event Calendar</h1>
          <p className="text-gray-600 mt-2">Discover upcoming concerts, conventions, and meet & greets.</p>
        </div>
        <h2 className="text-2xl font-semibold text-gray-700">October 2026</h2>
      </header>

      <section className="bg-white rounded-xl shadow p-6 overflow-x-auto">
        <div className="min-w-[800px]">
          {/* Calendar Header */}
          <div className="grid grid-cols-7 gap-2 mb-2 text-center font-semibold text-gray-600">
            <div>Sun</div><div>Mon</div><div>Tue</div><div>Wed</div><div>Thu</div><div>Fri</div><div>Sat</div>
          </div>
          
          {/* Calendar Grid */}
          <div className="grid grid-cols-7 gap-2">
            {/* Empty slots for start of month (Oct 2026 starts on Thursday) */}
            <div className="min-h-24 border rounded bg-gray-50/50"></div>
            <div className="min-h-24 border rounded bg-gray-50/50"></div>
            <div className="min-h-24 border rounded bg-gray-50/50"></div>
            <div className="min-h-24 border rounded bg-gray-50/50"></div>
            
            {daysInMonth.map(day => {
              // Oct 2026 string format
              const dateString = `2026-10-${day.toString().padStart(2, '0')}`;
              
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
