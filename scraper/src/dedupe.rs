//! The same event listed by several sources collapses into one record.
//! Two events match when they start the same day and either their titles are close,
//! or their venues share most words and the titles are at least loosely alike.
//! The earliest-registered source wins; its empty fields are filled from the others.
//! Absorbed events drop out of the batch, so the backend deletes their rows.
//! An artist's appearance at a listed event then joins that event's line-up.

use std::collections::{HashMap, HashSet};

use crate::{Event, IngestPayload};

const CLOSE_TITLE: f64 = 0.8;
const LOOSE_TITLE: f64 = 0.4;

/// Sørensen–Dice coefficient over character bigrams of the lowercase alphanumerics.
fn dice(a: &str, b: &str) -> f64 {
    let bigrams = |s: &str| {
        let c: Vec<char> = s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
        let mut m: HashMap<(char, char), usize> = HashMap::new();
        for w in c.windows(2) {
            *m.entry((w[0], w[1])).or_default() += 1;
        }
        m
    };
    let (a, b) = (bigrams(a), bigrams(b));
    let total: usize = a.values().chain(b.values()).sum();
    if total == 0 {
        return 0.0;
    }
    let shared: usize = a.iter().map(|(k, n)| (*n).min(b.get(k).copied().unwrap_or(0))).sum();
    2.0 * shared as f64 / total as f64
}

/// At least half the words of the shorter venue appear in the other.
fn same_venue(a: Option<&str>, b: Option<&str>) -> bool {
    let words = |s: &str| -> HashSet<String> {
        s.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 3).map(str::to_lowercase).collect()
    };
    let (Some(a), Some(b)) = (a.map(words), b.map(words)) else { return false };
    let shorter = a.len().min(b.len());
    shorter > 0 && a.intersection(&b).count() * 2 >= shorter
}

fn same_day(a: &Event, b: &Event) -> bool {
    a.start_date.get(..10) == b.start_date.get(..10)
}

fn same_event(a: &Event, b: &Event) -> bool {
    if !same_day(a, b) {
        return false;
    }
    let t = dice(&a.title, &b.title);
    t >= CLOSE_TITLE || (t >= LOOSE_TITLE && same_venue(a.location_name.as_deref(), b.location_name.as_deref()))
}

fn fill(w: &mut Event, l: Event) {
    w.series_id = w.series_id.take().or(l.series_id);
    w.location_name = w.location_name.take().or(l.location_name);
    w.location_city = w.location_city.take().or(l.location_city);
    w.floorplan_image_url = w.floorplan_image_url.take().or(l.floorplan_image_url);
    w.banner_image_url = w.banner_image_url.take().or(l.banner_image_url);
    w.official_url = w.official_url.take().or(l.official_url);
    w.description = w.description.take().or(l.description);
    w.organizer = w.organizer.take().or(l.organizer);
}

/// Merges duplicates in place. Artist appearances are left to `attach_appearances`.
pub fn merge_duplicates(payload: &mut IngestPayload) {
    let with_artists: HashSet<&str> = payload.event_artists.iter().map(|l| l.event_id.as_str()).collect();
    let events = &payload.events;
    let n = events.len();

    // Union-find where the root is always the lowest index, i.e. the highest-priority source.
    let mut root: Vec<usize> = (0..n).collect();
    fn find(root: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while root[r] != r {
            r = root[r];
        }
        root[i] = r;
        r
    }
    for i in 0..n {
        if with_artists.contains(events[i].id.as_str()) {
            continue;
        }
        for j in i + 1..n {
            if !with_artists.contains(events[j].id.as_str()) && same_event(&events[i], &events[j]) {
                let (ri, rj) = (find(&mut root, i), find(&mut root, j));
                root[ri.max(rj)] = ri.min(rj);
            }
        }
    }

    let mut slots: Vec<Option<Event>> = std::mem::take(&mut payload.events).into_iter().map(Some).collect();
    for i in 0..n {
        let r = find(&mut root, i);
        if r != i {
            let loser = slots[i].take().unwrap();
            let winner = slots[r].as_mut().unwrap();
            tracing::info!("Merged duplicate {:?} into {:?}", loser.title, winner.title);
            fill(winner, loser);
        }
    }
    payload.events = slots.into_iter().flatten().collect();
}

/// An appearance on an artist's own schedule that shares its start day and venue with a
/// listed event is that event: its artist links move there and the appearance drops out.
/// With several such events the closest title wins; without a venue it stays on its own.
pub fn attach_appearances(payload: &mut IngestPayload) {
    let appearances: HashSet<String> = payload.event_artists.iter().map(|l| l.event_id.clone()).collect();
    let mut moved: HashMap<String, String> = HashMap::new();

    for a in payload.events.iter().filter(|e| appearances.contains(&e.id)) {
        let candidates: Vec<&Event> = payload
            .events
            .iter()
            .filter(|e| !appearances.contains(&e.id) && same_day(a, e))
            .filter(|e| same_venue(a.location_name.as_deref(), e.location_name.as_deref()))
            .collect();
        let target = match candidates.as_slice() {
            [only] => Some(*only),
            many => many
                .iter()
                .map(|e| (dice(&a.title, &e.title), *e))
                .filter(|(t, _)| *t >= LOOSE_TITLE)
                .max_by(|x, y| x.0.total_cmp(&y.0))
                .map(|(_, e)| e),
        };
        if let Some(t) = target {
            tracing::info!("Attached appearance {:?} to {:?}", a.title, t.title);
            moved.insert(a.id.clone(), t.id.clone());
        }
    }

    for l in &mut payload.event_artists {
        if let Some(t) = moved.get(&l.event_id) {
            l.event_id = t.clone();
        }
    }
    payload.events.retain(|e| !moved.contains_key(&e.id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EventArtist;

    fn ev(id: &str, title: &str, start: &str, venue: Option<&str>, description: Option<&str>) -> Event {
        Event {
            id: id.to_string(),
            series_id: None,
            title: title.to_string(),
            category: "Test".to_string(),
            location_name: venue.map(str::to_string),
            location_city: None,
            floorplan_image_url: None,
            banner_image_url: None,
            official_url: None,
            start_date: format!("{start}T00:00:00+07:00"),
            end_date: format!("{start}T23:59:59+07:00"),
            description: description.map(str::to_string),
            organizer: None,
            source: String::new(),
        }
    }

    #[test]
    fn merges_real_duplicates() {
        let mut p = IngestPayload {
            events: vec![
                ev("jiexpo-sync", "SYNCHRONIZE FESTIVAL 2026", "2026-10-16", Some("JIExpo Kemayoran — Gambir Expo & Open Space"), None),
                ev("jiexpo-dwp", "Djakarta Warehouse Project 2026", "2026-12-11", Some("JIExpo Kemayoran — Hall A3, C2, D12 dan Open Space"), None),
                ev("ruang-jjm", "Jak Japan Matsuri 2026", "2026-10-31", Some("Plaza Parkir Timur, GBK Senayan"), None),
                ev("ruang-tokio", "Tokio Fest", "2026-10-10", Some("Chillax Sudirman"), None),
                ev("ef-sync", "Synchronize Fest 2026", "2026-10-16", Some("Gambir Expo, Kemayoran"), Some("Three days of music.")),
                ev("ef-dwp", "DWP 2026 Jakarta", "2026-12-11", Some("Jiexpo Kemayoran"), None),
                ev("ef-jjm", "JAK-JAPAN MATSURI 2026", "2026-10-31", Some("Plaza Parkir Timur GBK Senayan, Jakarta"), None),
                ev("ef-jjm2", "Jakarta Japan Matsuri 2026", "2026-10-31", Some("TBA"), None),
                ev("ef-tokio", "Tokio 2026", "2026-10-10", Some("Chillax Sudirman"), None),
            ],
            ..Default::default()
        };
        merge_duplicates(&mut p);

        let ids: Vec<_> = p.events.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["jiexpo-sync", "jiexpo-dwp", "ruang-jjm", "ruang-tokio"]);
        assert_eq!(p.events[0].title, "SYNCHRONIZE FESTIVAL 2026", "winner keeps its own fields");
        assert_eq!(p.events[0].description.as_deref(), Some("Three days of music."), "gaps filled from the loser");
    }

    #[test]
    fn keeps_distinct_events() {
        let mut p = IngestPayload {
            events: vec![
                ev("a", "Midori Festival 5", "2026-10-03", Some("Green Pramuka Square"), None),
                ev("b", "CRSL Land Festival 2026", "2026-10-03", Some("Mandala Krida Yogyakarta"), None),
                ev("c", "NUi High School Fest 2026", "2026-11-28", Some("Stadion Madya GBK, Jakarta"), None),
                ev("d", "JOYLAND FESTIVAL 2026", "2026-11-28", Some("GBK Parkir Timur Senayan"), None),
                ev("e", "Indonesia Comic Con 2026", "2026-10-03", None, None),
                ev("f", "Indonesia Comic Con 2026", "2027-10-02", None, None),
                ev("punipun", "Punipun: Guest Cosplayer at Itasha Domei", "2026-10-10", Some("QBIG BSD"), None),
                ev("g", "Itasha Domei 2026", "2026-10-10", Some("Qbig BSD City"), None),
            ],
            event_artists: vec![link("punipun", Some("Guest Cosplayer"))],
            ..Default::default()
        };
        merge_duplicates(&mut p);
        assert_eq!(p.events.len(), 8, "appearances are not duplicates");
    }

    fn link(event_id: &str, role: Option<&str>) -> EventArtist {
        EventArtist {
            event_id: event_id.to_string(),
            artist_id: "punipun-artist".to_string(),
            role: role.map(str::to_string),
            source: String::new(),
        }
    }

    // From the live Punipun, RuangCosplay and Eventfest listings.
    #[test]
    fn appearances_join_the_event_they_happen_at() {
        let mut p = IngestPayload {
            events: vec![
                ev("p-bday", "Punipun Birthday!", "2026-10-07", None, None),
                ev("p-itasha", "Punipun: Guest Cosplayer at Itasha Domei", "2026-10-10", Some("QBIG BSD"), None),
                ev("p-asus", "Punipun: ASUS ROG Event", "2026-10-16", None, None),
                ev("p-igx", "Punipun Booth & Special Performance at Indonesia Game Expo 2026", "2026-10-24", Some("ICE BSD"), None),
                ev("p-cf", "[Booth AH21-22] Punipun x MatchaMei Booth at COMIFURO 23 (Comic Frontier)", "2026-10-31", Some("ICE BSD"), None),
                ev("itasha", "Itasha Domei 2026", "2026-10-10", Some("Qbig BSD City"), None),
                ev("tokio", "Tokio 2026", "2026-10-10", Some("Chillax Sudirman"), None),
                ev("igx", "IGX Stage 3", "2026-10-24", Some("ICE BSD Hall 09-10"), None),
                ev("cf", "Comic Frontier 23", "2026-10-31", Some("ICE BSD City"), None),
                ev("jjm", "Jak Japan Matsuri 2026", "2026-10-31", Some("Plaza Parkir Timur, GBK Senayan"), None),
            ],
            event_artists: vec![
                link("p-bday", None),
                link("p-itasha", Some("Guest Cosplayer")),
                link("p-asus", None),
                link("p-igx", Some("Booth & Special Performance")),
                link("p-cf", Some("[Booth AH21-22] Punipun x MatchaMei Booth")),
            ],
            ..Default::default()
        };
        attach_appearances(&mut p);

        let ids: Vec<_> = p.events.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["p-bday", "p-asus", "itasha", "tokio", "igx", "cf", "jjm"]);
        let links: Vec<_> = p.event_artists.iter().map(|l| (l.event_id.as_str(), l.role.as_deref())).collect();
        assert_eq!(
            links,
            vec![
                ("p-bday", None),
                ("itasha", Some("Guest Cosplayer")),
                ("p-asus", None),
                ("igx", Some("Booth & Special Performance")),
                ("cf", Some("[Booth AH21-22] Punipun x MatchaMei Booth")),
            ]
        );
    }

    #[test]
    fn ambiguous_venue_needs_a_close_title() {
        let mut p = IngestPayload {
            events: vec![
                ev("p", "Punipun: Guest at Hall Show", "2026-10-24", Some("ICE BSD"), None),
                ev("x", "Indonesia Motor Show", "2026-10-24", Some("ICE BSD Hall 1"), None),
                ev("y", "Pet Expo", "2026-10-24", Some("ICE BSD Hall 5"), None),
            ],
            event_artists: vec![link("p", Some("Guest"))],
            ..Default::default()
        };
        attach_appearances(&mut p);
        assert_eq!(p.events.len(), 3, "two events at the venue and neither title is close: stay separate");
        assert_eq!(p.event_artists[0].event_id, "p");
    }
}
