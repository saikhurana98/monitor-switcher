use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct Event {
    pub at: SystemTime,
    pub message: String,
}

impl Event {
    #[must_use]
    pub fn age_string(&self, now: SystemTime) -> String {
        let dur = now.duration_since(self.at).unwrap_or(Duration::ZERO);
        format_age(dur)
    }
}

#[must_use]
pub fn format_age(dur: Duration) -> String {
    let s = dur.as_secs();
    if s < 60 {
        format!("{s}s ago")
    } else if s < 3_600 {
        format!("{}m ago", s / 60)
    } else if s < 86_400 {
        format!("{}h ago", s / 3_600)
    } else {
        format!("{}d ago", s / 86_400)
    }
}

#[derive(Debug)]
pub struct SharedState {
    paused: Mutex<bool>,
    last_profile: Mutex<Option<String>>,
    events: Mutex<VecDeque<Event>>,
    max_events: usize,
}

impl SharedState {
    #[must_use]
    pub fn new(max_events: usize) -> Arc<Self> {
        Arc::new(Self {
            paused: Mutex::new(false),
            last_profile: Mutex::new(None),
            events: Mutex::new(VecDeque::with_capacity(max_events)),
            max_events,
        })
    }

    pub fn paused(&self) -> bool {
        *self.paused.lock().unwrap()
    }

    pub fn set_paused(&self, value: bool) {
        *self.paused.lock().unwrap() = value;
    }

    pub fn toggle_paused(&self) -> bool {
        let mut g = self.paused.lock().unwrap();
        *g = !*g;
        *g
    }

    pub fn last_profile(&self) -> Option<String> {
        self.last_profile.lock().unwrap().clone()
    }

    pub fn set_last_profile(&self, profile: Option<String>) {
        *self.last_profile.lock().unwrap() = profile;
    }

    pub fn push_event(&self, message: impl Into<String>) {
        let mut q = self.events.lock().unwrap();
        if q.len() == self.max_events {
            q.pop_front();
        }
        q.push_back(Event {
            at: SystemTime::now(),
            message: message.into(),
        });
    }

    pub fn events_snapshot(&self) -> Vec<Event> {
        self.events.lock().unwrap().iter().cloned().collect()
    }

    pub fn clear_events(&self) {
        self.events.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_toggle() {
        let s = SharedState::new(5);
        assert!(!s.paused());
        assert!(s.toggle_paused());
        assert!(s.paused());
        s.set_paused(false);
        assert!(!s.paused());
    }

    #[test]
    fn events_ring_buffer() {
        let s = SharedState::new(3);
        for i in 0..5 {
            s.push_event(format!("event-{i}"));
        }
        let evs = s.events_snapshot();
        assert_eq!(evs.len(), 3);
        assert_eq!(evs[0].message, "event-2");
        assert_eq!(evs[2].message, "event-4");
    }

    #[test]
    fn last_profile() {
        let s = SharedState::new(1);
        assert!(s.last_profile().is_none());
        s.set_last_profile(Some("pc".into()));
        assert_eq!(s.last_profile().as_deref(), Some("pc"));
    }

    #[test]
    fn clear_events_empties_ring() {
        let s = SharedState::new(3);
        s.push_event("a");
        s.push_event("b");
        s.clear_events();
        assert!(s.events_snapshot().is_empty());
    }

    #[test]
    fn format_age_buckets() {
        assert_eq!(format_age(Duration::from_secs(0)), "0s ago");
        assert_eq!(format_age(Duration::from_secs(45)), "45s ago");
        assert_eq!(format_age(Duration::from_secs(120)), "2m ago");
        assert_eq!(format_age(Duration::from_secs(7_200)), "2h ago");
        assert_eq!(format_age(Duration::from_secs(172_800)), "2d ago");
    }

    #[test]
    fn event_age_string() {
        let now = SystemTime::now();
        let ev = Event {
            at: now - Duration::from_secs(90),
            message: "x".into(),
        };
        assert_eq!(ev.age_string(now), "1m ago");
    }
}
