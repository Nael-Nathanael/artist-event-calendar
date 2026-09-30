import React from 'react';

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
    const res = await fetch('http://127.0.0.1:8081/api/events', {
      next: { revalidate: 60 }, // ISR: Revalidate every 60 seconds
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

  return (
    <main className="min-h-screen p-8 bg-gray-50 text-gray-900">
      <header className="mb-8">
        <h1 className="text-4xl font-bold text-gray-800 tracking-tight">Artist Event Calendar</h1>
        <p className="text-gray-600 mt-2">Discover upcoming concerts, conventions, and meet & greets.</p>
      </header>

      <section className="bg-white rounded-xl shadow p-6">
        <h2 className="text-2xl font-semibold mb-4 border-b pb-2">Upcoming Events</h2>
        
        {events.length === 0 ? (
          <div className="text-gray-500 py-10 text-center">No events found. Waiting for scrapers to run...</div>
        ) : (
          <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
            {events.map((event) => (
              <div key={event.id} className="border rounded-lg p-4 hover:shadow-md transition-shadow bg-gray-50">
                <div className="flex justify-between items-start mb-2">
                  <span className="text-xs font-bold uppercase tracking-wider text-blue-600 bg-blue-100 px-2 py-1 rounded-full">
                    {event.category}
                  </span>
                </div>
                <h3 className="text-lg font-bold text-gray-900">{event.title}</h3>
                
                <div className="mt-4 text-sm text-gray-600 space-y-1">
                  <p className="flex items-center">
                    <span className="font-semibold w-20">Starts:</span> 
                    {new Date(event.start_date).toLocaleDateString()}
                  </p>
                  <p className="flex items-center">
                    <span className="font-semibold w-20">Ends:</span> 
                    {new Date(event.end_date).toLocaleDateString()}
                  </p>
                  {(event.location_city || event.location_name) && (
                    <p className="flex items-start mt-2">
                      <span className="font-semibold w-20 mt-1">Location:</span> 
                      <span className="flex-1">
                        {event.location_name} {event.location_name && event.location_city ? ',' : ''} {event.location_city}
                      </span>
                    </p>
                  )}
                </div>
                
                {event.official_url && (
                  <div className="mt-4 pt-4 border-t border-gray-200">
                    <a 
                      href={event.official_url} 
                      target="_blank" 
                      rel="noopener noreferrer"
                      className="text-blue-600 hover:text-blue-800 font-medium text-sm"
                    >
                      Visit Official Website &rarr;
                    </a>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </section>
    </main>
  );
}
