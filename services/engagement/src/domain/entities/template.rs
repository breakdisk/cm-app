use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationTemplate {
    pub id: uuid::Uuid,
    pub tenant_id: Option<uuid::Uuid>,
    pub template_id: String,
    pub channel: NotificationChannel,
    pub language: String,
    pub subject: Option<String>,
    pub body: String,
    pub variables: Vec<String>,
    pub is_active: bool,
}

impl NotificationTemplate {
    pub fn render(&self, vars: &serde_json::Value) -> Result<String, String> {
        let mut rendered = self.body.clone();
        for var in &self.variables {
            let placeholder = format!("{{{{{}}}}}", var);
            let value = vars.get(var)
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("Missing required variable: {}", var))?;
            rendered = rendered.replace(&placeholder, value);
        }
        Ok(rendered)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NotificationChannel {
    WhatsApp,
    Sms,
    Email,
    Push,
}

impl NotificationChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WhatsApp => "whatsapp",
            Self::Sms      => "sms",
            Self::Email    => "email",
            Self::Push     => "push",
        }
    }
}
