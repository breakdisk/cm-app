//! Masked calling between the two people on a job.
//!
//! The platform rings the person who asked, and when they answer dials the
//! other party with the platform's own number as the caller id. Neither side
//! ever sees the other's line, and the leg the platform places is the only one
//! either phone records.
//!
//! Off unless the deployment has a voice number and Twilio credentials. Off
//! means "say so", not "pretend": the apps fall back to a normal dial where
//! they have a number to dial, and hide the button where they do not.

use logisticos_errors::{AppError, AppResult};
use serde::Serialize;

const TWILIO_CALLS_URL: &str = "https://api.twilio.com/2010-04-01/Accounts";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum CallOutcome {
    /// The platform is ringing the caller now; answering dials the other side.
    Bridged { call_sid: String },
    /// This deployment has no voice number, so there is nothing to bridge with.
    NotConfigured,
    /// One of the two numbers is missing — most often no driver assigned yet.
    NoNumber,
}

pub struct MaskedCalls {
    account_sid: Option<String>,
    auth_token: Option<String>,
    voice_number: Option<String>,
    client: reqwest::Client,
}

impl MaskedCalls {
    /// `TWILIO_ACCOUNT_SID`, `TWILIO_AUTH_TOKEN` (already used for SMS) and
    /// `TWILIO_VOICE_NUMBER`. Any of them missing leaves calling off.
    pub fn from_env() -> Self {
        let non_empty = |key: &str| {
            std::env::var(key).ok().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
        };
        Self {
            account_sid: non_empty("TWILIO_ACCOUNT_SID"),
            auth_token: non_empty("TWILIO_AUTH_TOKEN"),
            voice_number: non_empty("TWILIO_VOICE_NUMBER"),
            client: reqwest::Client::new(),
        }
    }

    pub fn configured(&self) -> bool {
        self.account_sid.is_some() && self.auth_token.is_some() && self.voice_number.is_some()
    }

    /// The number both phones see. Shown to each side so the masked line is
    /// recognisable when it rings.
    pub fn masked_number(&self) -> Option<&str> {
        self.voice_number.as_deref()
    }

    pub async fn bridge(&self, caller: Option<&str>, callee: Option<&str>) -> AppResult<CallOutcome> {
        let (Some(account_sid), Some(auth_token), Some(voice_number)) =
            (&self.account_sid, &self.auth_token, &self.voice_number)
        else {
            return Ok(CallOutcome::NotConfigured);
        };

        let (Some(caller), Some(callee)) = (caller, callee) else {
            return Ok(CallOutcome::NoNumber);
        };
        if !is_dialable(caller) || !is_dialable(callee) {
            return Ok(CallOutcome::NoNumber);
        }

        let twiml = dial_twiml(voice_number, callee);
        let response = self
            .client
            .post(format!("{TWILIO_CALLS_URL}/{account_sid}/Calls.json"))
            .basic_auth(account_sid, Some(auth_token))
            .form(&[
                ("To", caller),
                ("From", voice_number.as_str()),
                ("Twiml", twiml.as_str()),
            ])
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Twilio unreachable: {e}")))?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::Internal(anyhow::anyhow!("Twilio answered {status}: {body}")));
        }

        let call_sid = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("sid").and_then(|s| s.as_str().map(str::to_owned)))
            .unwrap_or_default();
        Ok(CallOutcome::Bridged { call_sid })
    }
}

/// A number worth dialling: `+` and 7 to 15 digits, which is E.164's range.
/// This is also what keeps anything but digits out of the TwiML below.
pub fn is_dialable(number: &str) -> bool {
    let trimmed = number.trim();
    let digits = trimmed.strip_prefix('+').unwrap_or(trimmed);
    !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
        && (7..=15).contains(&digits.chars().count())
}

/// The instruction for the answered leg: dial the other party, showing the
/// platform's number. Both values are checked by [`is_dialable`] first, so
/// nothing that could close a tag reaches this.
fn dial_twiml(caller_id: &str, callee: &str) -> String {
    format!("<Response><Dial callerId=\"{caller_id}\"><Number>{callee}</Number></Dial></Response>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dialable_number_is_e164_shaped() {
        assert!(is_dialable("+639171234567"));
        assert!(is_dialable("639171234567"));
        assert!(is_dialable(" +14155550123 "));
    }

    #[test]
    fn anything_else_is_not_dialled() {
        assert!(!is_dialable(""));
        assert!(!is_dialable("+"));
        assert!(!is_dialable("123456"), "six digits is too short for E.164");
        assert!(!is_dialable("+1234567890123456"), "sixteen digits is too long");
        assert!(!is_dialable("+63 917 123 4567"), "spaces are not stripped for us");
        assert!(!is_dialable("+63917123456a"));
    }

    /// The guard that matters: a number carrying markup would otherwise become
    /// TwiML of the caller's choosing.
    #[test]
    fn markup_can_never_reach_the_twiml() {
        assert!(!is_dialable("\"/><Dial>+15005550006</Dial><!--"));
        let twiml = dial_twiml("+15005550006", "+639171234567");
        assert_eq!(
            twiml,
            "<Response><Dial callerId=\"+15005550006\"><Number>+639171234567</Number></Dial></Response>"
        );
    }

    #[test]
    fn without_credentials_nothing_is_bridged() {
        let calls = MaskedCalls { account_sid: None, auth_token: None, voice_number: None, client: reqwest::Client::new() };
        assert!(!calls.configured());
        assert_eq!(calls.masked_number(), None);
    }
}
