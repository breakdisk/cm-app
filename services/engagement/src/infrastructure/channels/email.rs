use async_trait::async_trait;
use aws_sdk_sesv2::{
    types::{Body, Content, Destination, EmailContent, Message},
    Client as SesClient,
};
use super::ChannelAdapter;

pub struct SesEmailAdapter {
    client: SesClient,
    from_email: String,
    from_name: String,
}

impl SesEmailAdapter {
    pub async fn new(from_email: String, from_name: String) -> Self {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = SesClient::new(&config);
        Self { client, from_email, from_name }
    }
}

#[async_trait]
impl ChannelAdapter for SesEmailAdapter {
    async fn send(&self, recipient: &str, body: &str, subject: Option<&str>) -> Result<String, String> {
        let from = format!("{} <{}>", self.from_name, self.from_email);
        let subject_content = Content::builder()
            .data(subject.unwrap_or("LogisticOS Notification"))
            .charset("UTF-8")
            .build()
            .map_err(|e| format!("SES subject build: {e}"))?;
        let body_content = Content::builder()
            .data(body)
            .charset("UTF-8")
            .build()
            .map_err(|e| format!("SES body build: {e}"))?;
        let message = Message::builder()
            .subject(subject_content)
            .body(Body::builder().html(body_content).build())
            .build();
        let email_content = EmailContent::builder().simple(message).build();
        let destination = Destination::builder().to_addresses(recipient).build();

        let result = self.client
            .send_email()
            .from_email_address(from)
            .destination(destination)
            .content(email_content)
            .send()
            .await
            .map_err(|e| format!("SES send failed: {e}"))?;

        Ok(result.message_id().unwrap_or("unknown").to_owned())
    }
}
