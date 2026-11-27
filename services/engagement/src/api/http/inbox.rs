//! The customer's campaign inbox: every campaign message they were sent, on
//! any channel, kept for them to read in the app.
//!
//! No permission gate and no `customer_id` parameter: the inbox is always the
//! caller's own. A send is theirs when it was addressed to their user id, or
//! went to the email or phone on their login. Campaign recipients are CDP
//! profiles, whose ids are not user ids, so the address is what usually
//! matches.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;

use crate::{
    api::http::AppState,
    infrastructure::db::inbox::{InboxDb, Reader},
};

/// Longest body the inbox keeps. A campaign email can be a whole page of
/// HTML; the inbox is a message list.
const BODY_MAX_CHARS: usize = 2_000;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

/// What the inbox shows for one send: the subject, or the campaign's name
/// when the channel has none (push, SMS); and the body as plain text.
pub fn entry_for(subject: Option<&str>, campaign_name: &str, rendered_body: &str) -> (String, String) {
    let title = subject
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| campaign_name.trim())
        .to_owned();
    let title = if title.is_empty() { "Message".to_owned() } else { title };
    let body: String = plain_text(rendered_body).chars().take(BODY_MAX_CHARS).collect();
    (title, body)
}

/// The one form an address is stored and matched in: an email lowercased, a
/// phone as its digits. None for anything that is neither.
pub fn normalise_address(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.contains('@') {
        return Some(raw.to_lowercase()).filter(|e| e.len() > 3);
    }
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    // Seven digits is the shortest real subscriber number; anything shorter
    // is not a phone and must not match someone else's fragment.
    (digits.len() >= 7).then_some(digits)
}

fn reader(claims: &AuthClaims) -> Reader {
    let addresses = std::iter::once(claims.email.as_str())
        .chain(claims.phone.as_deref())
        .filter_map(normalise_address)
        .collect();
    Reader { tenant_id: claims.tenant_id, user_id: claims.user_id, addresses }
}

/// Tags dropped, common entities decoded, runs of whitespace folded. Enough
/// for an email body to read as text; not an HTML parser.
fn plain_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Only `<` that opens a tag: "2 < 3" in a text body is kept.
            '<' if !in_tag && chars.peek().is_some_and(|n| n.is_ascii_alphabetic() || *n == '/' || *n == '!') => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            _ => out.push(c),
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `GET /v1/engagement/inbox`
pub async fn list(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    let db = InboxDb::new(state.db.clone());
    let who = reader(&claims);
    let items = db
        .list(&who, q.before, q.limit.unwrap_or(50).clamp(1, 100))
        .await
        .map_err(AppError::Internal)?;
    let unread = db.unread(&who).await.map_err(AppError::Internal)?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": { "unread": unread, "items": items } }))))
}

/// `POST /v1/engagement/inbox/:id/read`
pub async fn mark_read(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let found = InboxDb::new(state.db.clone())
        .mark_read(&reader(&claims), id, Utc::now())
        .await
        .map_err(AppError::Internal)?;
    if !found {
        return Err(AppError::NotFound { resource: "Inbox message", id: id.to_string() });
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/engagement/inbox/read-all`
pub async fn mark_all_read(State(state): State<AppState>, claims: AuthClaims) -> impl IntoResponse {
    let marked = InboxDb::new(state.db.clone())
        .mark_all_read(&reader(&claims), Utc::now())
        .await
        .map_err(AppError::Internal)?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": { "marked": marked } }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subject_titles_the_message_and_the_campaign_name_stands_in_for_none() {
        assert_eq!(entry_for(Some("Weekend 20% off"), "Sept promo", "Hi").0, "Weekend 20% off");
        assert_eq!(entry_for(None, "Sept promo", "Hi").0, "Sept promo");
        assert_eq!(entry_for(Some("  "), "Sept promo", "Hi").0, "Sept promo");
        assert_eq!(entry_for(None, " ", "Hi").0, "Message");
    }

    #[test]
    fn addresses_match_in_one_form() {
        assert_eq!(normalise_address(" Ana@Example.COM ").as_deref(), Some("ana@example.com"));
        assert_eq!(normalise_address("+63 917-555-0123").as_deref(), Some("639175550123"));
        assert_eq!(normalise_address("+639175550123"), normalise_address("+63 917 555 0123"));
        assert_eq!(normalise_address("12"), None, "a fragment must not match anyone's number");
        assert_eq!(normalise_address(""), None);
    }

    #[test]
    fn an_email_body_reads_as_text() {
        let (_, body) = entry_for(
            Some("Hello"),
            "c",
            "<html><body><p>Hi&nbsp;Ana,</p>\n<p>Use <b>MOVE20</b> &amp; save.</p></body></html>",
        );
        assert_eq!(body, "Hi Ana, Use MOVE20 & save.");
    }

    #[test]
    fn a_plain_body_is_kept_and_a_long_one_is_cut() {
        assert_eq!(entry_for(None, "c", "2 < 3 and 5 > 4").1, "2 < 3 and 5 > 4");
        let long = "a".repeat(BODY_MAX_CHARS + 50);
        assert_eq!(entry_for(None, "c", &long).1.chars().count(), BODY_MAX_CHARS);
    }
}
