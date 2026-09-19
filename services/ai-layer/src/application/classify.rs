//! Intent routing for the Move app's prompt box: is this a booking, a
//! whole-home move, or a question about a job already running — and if a
//! booking, what did they say.
//!
//! Stateless on purpose. The prompt box needs the answer before the thinking
//! screen, and a full agent session per send would fill the agents dashboard
//! with one-line non-conversations. No session is created and no tool runs:
//! the model is asked for one structured answer, which is then held to rules
//! it cannot bend — the intents are a closed set, the numbers are clamped, and
//! "support" needs a job to be about.

use logisticos_agent_runtime::claude::{ClaudeApi, ContentBlock};
use logisticos_agent_runtime::session::{AgentMessage, MessageRole};
use logisticos_agent_runtime::tools::ToolDefinition;
use serde::Serialize;
use serde_json::{json, Value};

const TOOL: &str = "route_prompt";
const MAX_ITEMS: usize = 30;
const MAX_FIELD_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    /// A load moved from A to B: a sofa, a few boxes.
    Book,
    /// A whole home or office: rooms, a survey, a crew.
    HomeMove,
    /// A question about a job already booked.
    Support,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    pub name: String,
    pub qty: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Extracted {
    pub items: Vec<Item>,
    pub from: String,
    pub to: String,
    /// As they said it ("tomorrow at 9am"); the plan screen schedules it.
    pub when: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Classification {
    pub intent: Intent,
    /// 0–1. The app asks the customer to confirm below its own threshold.
    pub confidence: f64,
    pub extracted: Extracted,
}

const SYSTEM: &str = "You route one message typed into a moving app's prompt box. \
Call route_prompt exactly once. intent is book (moving some items), home_move (moving a whole \
home, flat or office — rooms, a household, 'everything'), or support (a question or problem about \
a move already booked: where is my driver, change the time, cancel, a complaint). \
Extract only what the message says: never invent an address, a time or an item. \
Quantities are whole numbers; 'a couch' is 1. Leave a field empty when it is not stated.";

pub fn tool() -> ToolDefinition {
    ToolDefinition {
        name: TOOL.into(),
        description: "Route the prompt and extract what it states.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "intent": { "type": "string", "enum": ["book", "home_move", "support"] },
                "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": { "name": { "type": "string" }, "qty": { "type": "integer", "minimum": 1 } },
                        "required": ["name", "qty"]
                    }
                },
                "from": { "type": "string" },
                "to": { "type": "string" },
                "when": { "type": "string" }
            },
            "required": ["intent", "confidence"]
        }),
    }
}

fn text_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .map(|s| s.chars().take(MAX_FIELD_CHARS).collect())
        .unwrap_or_default()
}

/// The model's answer, held to the rules. Anything unreadable is a booking
/// with nothing extracted and no confidence — the app then falls back to
/// its own parse, which is what it does offline anyway.
pub fn normalise(input: &Value, has_active_job: bool) -> Classification {
    let mut intent = match input.get("intent").and_then(Value::as_str) {
        Some("home_move") => Intent::HomeMove,
        Some("support") => Intent::Support,
        _ => Intent::Book,
    };
    // A question needs a job to be about. With none running, the same words
    // are a request to book — the rule the app's offline classifier applies.
    if intent == Intent::Support && !has_active_job {
        intent = Intent::Book;
    }
    let confidence = input
        .get("confidence")
        .and_then(Value::as_f64)
        .filter(|c| c.is_finite())
        .map_or(0.0, |c| c.clamp(0.0, 1.0));

    let items = input
        .get("items")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|i| {
                    let name = text_field(i, "name");
                    let qty = i.get("qty").and_then(Value::as_u64).unwrap_or(1).clamp(1, 99);
                    (!name.is_empty()).then(|| Item { name, qty: u32::try_from(qty).unwrap_or(1) })
                })
                .take(MAX_ITEMS)
                .collect()
        })
        .unwrap_or_default();

    Classification {
        intent,
        confidence,
        extracted: Extracted { items, from: text_field(input, "from"), to: text_field(input, "to"), when: text_field(input, "when") },
    }
}

pub async fn classify(api: &dyn ClaudeApi, text: &str, has_active_job: bool) -> anyhow::Result<Classification> {
    let messages = [AgentMessage { role: MessageRole::User, content: Value::String(text.to_owned()) }];
    let response = api.send(SYSTEM, &messages, &[tool()]).await?;
    let answer = response.content.iter().find_map(|b| match b {
        ContentBlock::ToolUse { name, input, .. } if name == TOOL => Some(input),
        _ => None,
    });
    match answer {
        Some(input) => Ok(normalise(input, has_active_job)),
        None => anyhow::bail!("the model answered without routing the prompt"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_agent_runtime::testing::StubClaude;

    #[test]
    fn the_answer_is_held_to_the_rules() {
        let c = normalise(
            &json!({
                "intent": "book", "confidence": 1.7,
                "items": [{ "name": "  sofa ", "qty": 0 }, { "name": "", "qty": 3 }, { "name": "boxes", "qty": 500 }],
                "from": "12 Elm St", "to": "Makati", "when": "tomorrow 9am"
            }),
            false,
        );
        assert_eq!(c.intent, Intent::Book);
        assert!((c.confidence - 1.0).abs() < f64::EPSILON);
        assert_eq!(c.extracted.items, vec![Item { name: "sofa".into(), qty: 1 }, Item { name: "boxes".into(), qty: 99 }]);
        assert_eq!(c.extracted.to, "Makati");
    }

    #[test]
    fn support_needs_a_job_to_be_about() {
        let asked = json!({ "intent": "support", "confidence": 0.9 });
        assert_eq!(normalise(&asked, false).intent, Intent::Book);
        assert_eq!(normalise(&asked, true).intent, Intent::Support);
    }

    #[test]
    fn an_unknown_intent_or_junk_is_a_booking_with_no_confidence() {
        let c = normalise(&json!({ "intent": "launch_rocket", "confidence": "high" }), true);
        assert_eq!(c.intent, Intent::Book);
        assert!(c.confidence.abs() < f64::EPSILON);
        assert_eq!(c.extracted, Extracted::default());
    }

    #[test]
    fn a_whole_home_is_its_own_intent() {
        assert_eq!(normalise(&json!({ "intent": "home_move", "confidence": 0.8 }), false).intent, Intent::HomeMove);
    }

    #[tokio::test]
    async fn the_routing_tool_call_is_what_is_read() {
        let stub = StubClaude::new(vec![StubClaude::tool_call(
            "t1",
            TOOL,
            json!({ "intent": "home_move", "confidence": 0.84, "from": "BGC" }),
        )]);
        let c = classify(&stub, "moving my 2 bedroom flat from BGC", false).await.unwrap();
        assert_eq!(c.intent, Intent::HomeMove);
        assert_eq!(c.extracted.from, "BGC");
    }

    #[tokio::test]
    async fn prose_instead_of_a_routing_call_is_an_error_the_app_falls_back_from() {
        let stub = StubClaude::new(vec![StubClaude::text("Sure! Happy to help you move.")]);
        assert!(classify(&stub, "move my sofa", false).await.is_err());
    }
}
