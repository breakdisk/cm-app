//! Anthropic Claude API client for Rust services.
//!
//! Wraps the Anthropic Messages API with typed request/response structs.
//! The ai-layer Python service handles full agentic orchestration;
//! this crate is for lightweight Claude calls from Rust services
//! (e.g. generating notification copy, summarizing a route, fraud scoring explanations).

use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role:    Role,
    pub content: String,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into() }
    }
}

#[derive(Debug, Serialize)]
struct MessagesRequest<'a> {
    model:       &'a str,
    max_tokens:  u32,
    system:      Option<&'a str>,
    messages:    &'a [Message],
}

#[derive(Debug, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text:       Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MessagesResponse {
    pub id:      String,
    pub content: Vec<ContentBlock>,
    pub model:   String,
    pub usage:   Usage,
}

impl MessagesResponse {
    /// Extract the first text content block.
    pub fn text(&self) -> Option<&str> {
        self.content.iter()
            .find(|b| b.block_type == "text")
            .and_then(|b| b.text.as_deref())
    }
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub input_tokens:  u32,
    pub output_tokens: u32,
}

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ClaudeError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error {status}: {message}")]
    Api { status: u16, message: String },
    #[error("No text content in response")]
    EmptyResponse,
}

// ── Client ────────────────────────────────────────────────────────────────────

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL:     &str = "claude-haiku-4-5-20251001"; // fast + cheap for inline use
const DEFAULT_MAX_TOKENS: u32 = 1024;

#[derive(Clone)]
pub struct ClaudeClient {
    http:      reqwest::Client,
    api_key:   String,
    model:     String,
    max_tokens: u32,
}

impl ClaudeClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http:       reqwest::Client::new(),
            api_key:    api_key.into(),
            model:      DEFAULT_MODEL.to_string(),
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Single-turn completion.
    pub async fn complete(
        &self,
        system:  Option<&str>,
        user_msg: &str,
    ) -> Result<String, ClaudeError> {
        let messages = vec![Message::user(user_msg)];
        let resp = self.messages(system, &messages).await?;
        resp.text().map(str::to_owned).ok_or(ClaudeError::EmptyResponse)
    }

    /// Multi-turn messages call.
    pub async fn messages(
        &self,
        system:   Option<&str>,
        messages: &[Message],
    ) -> Result<MessagesResponse, ClaudeError> {
        let body = MessagesRequest {
            model:      &self.model,
            max_tokens: self.max_tokens,
            system,
            messages,
        };

        let resp = self.http
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(ClaudeError::Api { status, message });
        }

        Ok(resp.json::<MessagesResponse>().await?)
    }
}

// ── Convenience builders ──────────────────────────────────────────────────────

/// Build a Claude client from ANTHROPIC_API_KEY env var.
/// Panics at startup if not set — fail-fast is preferred over silent degradation.
pub fn from_env() -> ClaudeClient {
    let key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY env var required");
    ClaudeClient::new(key)
}

/// Pre-built system prompt for logistics notification copy generation.
pub const NOTIFICATION_SYSTEM: &str = r#"
You are a logistics communication assistant for LogisticOS.
Generate short, clear delivery notification messages in the requested language.
Be concise (1-2 sentences max). Always include the tracking number if provided.
Tone: friendly and professional. Never use jargon.
"#;

/// Pre-built system prompt for fraud risk explanation.
pub const FRAUD_EXPLANATION_SYSTEM: &str = r#"
You are a fraud risk analyst for a logistics platform.
Given a risk score and signal list, provide a 1-sentence plain-English explanation
suitable for an operations team member. Do not reveal internal scoring weights.
"#;
