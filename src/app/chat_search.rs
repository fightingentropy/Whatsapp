//! Local conversation search, adapted from ZapFast's search inspector.

use std::time::{Duration, Instant};

use jiff::{civil::Date, tz::TimeZone};

use super::App;
use crate::backend::Command;
use crate::model::Message;

#[derive(Default)]
pub struct ChatSearch {
    pub chat: Option<String>,
    pub query: String,
    pub day: Option<Date>,
    pub hits: Vec<Message>,
    pub selected: Option<usize>,
    pub focus: bool,
    pub pending: bool,
    pub truncated: bool,
    pub error: Option<String>,
    request: u64,
    due: Option<Instant>,
}

impl ChatSearch {
    pub fn close(&mut self) {
        let request = self.request.wrapping_add(1);
        *self = Self {
            request,
            ..Self::default()
        };
    }

    pub fn changed(&mut self) {
        self.request = self.request.wrapping_add(1);
        self.hits.clear();
        self.selected = None;
        self.error = None;
        self.truncated = false;
        self.pending = !self.query.trim().is_empty() || self.day.is_some();
        self.due = self
            .pending
            .then(|| Instant::now() + Duration::from_millis(180));
    }

    /// Searches the same query again after the chat's messages changed,
    /// keeping the current results and selection on screen meanwhile.
    pub fn refresh(&mut self) {
        if self.query.trim().is_empty() && self.day.is_none() {
            return;
        }
        self.request = self.request.wrapping_add(1);
        // A steady stream of changes cannot postpone the search forever.
        self.due = Some(
            self.due
                .unwrap_or_else(|| Instant::now() + Duration::from_millis(180)),
        );
    }

    pub fn accept(
        &mut self,
        chat: &str,
        request: u64,
        result: Result<Vec<Message>, String>,
        truncated: bool,
    ) {
        if self.chat.as_deref() != Some(chat) || request != self.request {
            return;
        }
        self.pending = false;
        self.truncated = truncated;
        match result {
            Ok(hits) => {
                let keep = self
                    .selected
                    .and_then(|index| self.hits.get(index))
                    .map(|hit| hit.id.clone());
                self.hits = hits;
                self.selected = keep.and_then(|id| self.hits.iter().position(|hit| hit.id == id));
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn step(&mut self, step: i32) -> Option<String> {
        let count = self.hits.len();
        if count == 0 {
            return None;
        }
        let index = match self.selected {
            Some(index) => (index as i64 + i64::from(step)).rem_euclid(count as i64) as usize,
            None if step < 0 => count - 1,
            None => 0,
        };
        self.selected = Some(index);
        Some(self.hits[index].id.clone())
    }
}

/// Midnight to midnight in the user's time zone, including 23/25-hour days.
pub fn day_range(day: Date, zone: TimeZone) -> Result<(i64, i64), jiff::Error> {
    let start = day.to_zoned(zone.clone())?.timestamp().as_second();
    let end = day.tomorrow()?.to_zoned(zone)?.timestamp().as_second();
    Ok((start, end))
}

impl App {
    pub(super) fn pump_chat_search(&mut self, now: Instant, ctx: &egui::Context) {
        let Some(due) = self.chat_search.due else {
            return;
        };
        if now < due {
            ctx.request_repaint_after(due - now);
            return;
        }
        self.chat_search.due = None;
        let Some(chat) = self.chat_search.chat.clone() else {
            return;
        };
        let day = match self
            .chat_search
            .day
            .map(|day| day_range(day, TimeZone::system()))
            .transpose()
        {
            Ok(day) => day,
            Err(_) => {
                self.chat_search.pending = false;
                self.chat_search.error = Some("This date could not be searched.".into());
                return;
            }
        };
        self.backend.send(Command::SearchChat {
            chat,
            query: self.chat_search.query.trim().to_owned(),
            day,
            request: self.chat_search.request,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_ranges_follow_daylight_saving_time() {
        let zone = TimeZone::get("Europe/London").unwrap();
        for (date, hours) in [("2026-03-29", 23), ("2026-10-25", 25), ("2026-09-24", 24)] {
            let (start, end) = day_range(date.parse().unwrap(), zone.clone()).unwrap();
            assert_eq!(end - start, hours * 3600);
        }
    }

    fn hit(id: &str) -> Message {
        Message {
            id: id.into(),
            chat: "chat".into(),
            sender: "chat".into(),
            sender_name: None,
            from_me: false,
            timestamp: 1,
            content: crate::model::Content::text(id),
            status: crate::model::Delivery::None,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        }
    }

    #[test]
    fn a_refresh_keeps_results_and_follows_the_selected_hit() {
        let mut search = ChatSearch {
            chat: Some("chat".into()),
            query: "fixture".into(),
            hits: vec![hit("a"), hit("b"), hit("c")],
            selected: Some(1),
            ..Default::default()
        };
        search.refresh();
        assert_eq!(search.hits.len(), 3);
        assert_eq!(search.selected, Some(1));
        assert!(!search.pending, "the results stay on screen");
        let due = search.due.expect("a new search is scheduled");
        search.refresh();
        assert_eq!(search.due, Some(due), "later changes do not postpone it");
        let request = search.request;
        search.accept(
            "chat",
            request,
            Ok(vec![hit("x"), hit("a"), hit("b"), hit("c")]),
            false,
        );
        assert_eq!(search.hits.len(), 4);
        assert_eq!(search.selected, Some(2), "the same message stays selected");
        search.accept("chat", request, Ok(vec![hit("a")]), false);
        assert_eq!(search.selected, None, "a vanished hit is unselected");

        let mut idle = ChatSearch {
            chat: Some("chat".into()),
            ..Default::default()
        };
        idle.refresh();
        assert!(idle.due.is_none(), "nothing to search again");
    }

    #[test]
    fn older_replies_cannot_replace_a_new_query_or_reopened_search() {
        let mut search = ChatSearch {
            chat: Some("chat".into()),
            query: "first".into(),
            ..Default::default()
        };
        search.changed();
        let first = search.request;
        search.query = "second".into();
        search.changed();
        search.accept("chat", first, Err("stale".into()), true);
        assert!(search.error.is_none() && search.pending && !search.truncated);
        let second = search.request;
        search.close();
        search.chat = Some("chat".into());
        search.accept("chat", second, Err("stale".into()), true);
        assert!(search.error.is_none() && !search.truncated);
    }
}
