//! Job chat: the thread between the customer and the driver on one shipment.
//!
//! Who may read or post is decided against the service that owns the fact —
//! order-intake for "this is my shipment", driver-ops for "I am the driver on
//! it" (see `infrastructure::external::job_participants`). This module holds
//! only what is true regardless of that: what a message is, what a body may
//! contain, and what counts as unread.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Long enough for directions to a back entrance, short enough that the column
/// check and the phone agree on the limit.
pub const MAX_BODY_CHARS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SenderRole {
    Customer,
    Driver,
}

impl SenderRole {
    pub fn as_str(self) -> &'static str {
        match self {
            SenderRole::Customer => "customer",
            SenderRole::Driver => "driver",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "customer" => Some(SenderRole::Customer),
            "driver" => Some(SenderRole::Driver),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct JobMessage {
    pub id: Uuid,
    pub shipment_id: Uuid,
    pub sender_id: Uuid,
    pub sender_role: SenderRole,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyError {
    Empty,
    TooLong,
}

impl BodyError {
    pub fn message(self) -> String {
        match self {
            BodyError::Empty => "A message needs something in it.".to_owned(),
            BodyError::TooLong => format!("A message can be at most {MAX_BODY_CHARS} characters."),
        }
    }
}

/// Trims, refuses an empty message, and measures the cap in characters rather
/// than bytes — so an emoji costs one, as it does on the phone that typed it,
/// and the column's `char_length` check agrees with this function.
pub fn clean_body(raw: &str) -> Result<String, BodyError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(BodyError::Empty);
    }
    if trimmed.chars().count() > MAX_BODY_CHARS {
        return Err(BodyError::TooLong);
    }
    Ok(trimmed.to_owned())
}

/// Unread means: from the other person, after the last time this one read.
/// Your own messages are never unread, however long ago you sent them.
pub fn unread_count(messages: &[JobMessage], me: Uuid, last_read: Option<DateTime<Utc>>) -> usize {
    messages
        .iter()
        .filter(|m| m.sender_id != me)
        .filter(|m| last_read.is_none_or(|read_at| m.created_at > read_at))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 16, 9, minute, 0).unwrap()
    }

    fn message(sender: Uuid, minute: u32) -> JobMessage {
        JobMessage {
            id: Uuid::new_v4(),
            shipment_id: Uuid::new_v4(),
            sender_id: sender,
            sender_role: SenderRole::Driver,
            body: "On my way".to_owned(),
            created_at: at(minute),
        }
    }

    #[test]
    fn a_body_is_trimmed_and_kept() {
        assert_eq!(clean_body("  gate code is 4417  ").unwrap(), "gate code is 4417");
    }

    #[test]
    fn an_empty_or_blank_body_is_refused() {
        assert_eq!(clean_body(""), Err(BodyError::Empty));
        assert_eq!(clean_body("   \n\t "), Err(BodyError::Empty));
    }

    /// The cap counts characters, not bytes: 2000 emoji are 2000 characters on
    /// the phone and must be 2000 here, or the column check refuses a message
    /// the app said was fine.
    #[test]
    fn the_cap_counts_characters_not_bytes() {
        let emoji = "🛋".repeat(MAX_BODY_CHARS);
        assert!(emoji.len() > MAX_BODY_CHARS, "this test is pointless unless the bytes exceed the cap");
        assert!(clean_body(&emoji).is_ok());
        assert_eq!(clean_body(&"a".repeat(MAX_BODY_CHARS + 1)), Err(BodyError::TooLong));
    }

    #[test]
    fn roles_survive_a_round_trip() {
        for role in [SenderRole::Customer, SenderRole::Driver] {
            assert_eq!(SenderRole::parse(role.as_str()), Some(role));
        }
        assert_eq!(SenderRole::parse("dispatcher"), None);
    }

    #[test]
    fn unread_is_what_the_other_person_sent_since_you_last_read() {
        let me = Uuid::new_v4();
        let them = Uuid::new_v4();
        let thread = vec![message(them, 1), message(me, 2), message(them, 3), message(them, 4)];

        assert_eq!(unread_count(&thread, me, None), 3);
        assert_eq!(unread_count(&thread, me, Some(at(2))), 2);
        assert_eq!(unread_count(&thread, me, Some(at(4))), 0);
    }

    #[test]
    fn your_own_messages_are_never_unread() {
        let me = Uuid::new_v4();
        let thread = vec![message(me, 1), message(me, 5)];
        assert_eq!(unread_count(&thread, me, None), 0);
    }
}
