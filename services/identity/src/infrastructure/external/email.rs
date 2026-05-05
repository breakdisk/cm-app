//! AWS SES v2 email adapter.
//! Falls back to tracing::info! when EMAIL__FROM_ADDRESS is not configured (dev/test).

use async_trait::async_trait;

#[async_trait]
pub trait EmailAdapter: Send + Sync {
    async fn send(&self, to: &str, subject: &str, html_body: &str) -> anyhow::Result<()>;
}

/// SES v2 implementation.
pub struct SesEmailAdapter {
    client: aws_sdk_sesv2::Client,
    from_address: String,
}

impl SesEmailAdapter {
    pub async fn new(from_address: String, region: Option<String>) -> anyhow::Result<Self> {
        let region_str = region
            .or_else(|| std::env::var("AWS_REGION").ok())
            .unwrap_or_else(|| "us-east-1".to_string());
        let aws_region = aws_sdk_sesv2::config::Region::new(region_str);
        let sdk_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_region)
            .load()
            .await;
        let client = aws_sdk_sesv2::Client::new(&sdk_config);
        Ok(Self { client, from_address })
    }
}

#[async_trait]
impl EmailAdapter for SesEmailAdapter {
    async fn send(&self, to: &str, subject: &str, html_body: &str) -> anyhow::Result<()> {
        self.client
            .send_email()
            .from_email_address(&self.from_address)
            .destination(
                aws_sdk_sesv2::types::Destination::builder()
                    .to_addresses(to)
                    .build(),
            )
            .content(
                aws_sdk_sesv2::types::EmailContent::builder()
                    .simple(
                        aws_sdk_sesv2::types::Message::builder()
                            .subject(
                                aws_sdk_sesv2::types::Content::builder()
                                    .data(subject)
                                    .charset("UTF-8")
                                    .build()?,
                            )
                            .body(
                                aws_sdk_sesv2::types::Body::builder()
                                    .html(
                                        aws_sdk_sesv2::types::Content::builder()
                                            .data(html_body)
                                            .charset("UTF-8")
                                            .build()?,
                                    )
                                    .build(),
                            )
                            .build(),
                    )
                    .build(),
            )
            .send()
            .await?;
        Ok(())
    }
}

/// No-op adapter used when SES is not configured — logs to tracing::info!.
pub struct LogEmailAdapter;

#[async_trait]
impl EmailAdapter for LogEmailAdapter {
    async fn send(&self, to: &str, subject: &str, html_body: &str) -> anyhow::Result<()> {
        tracing::info!(to, subject, body_len = html_body.len(), "[DEV] email not sent — configure EMAIL__FROM_ADDRESS to enable SES");
        Ok(())
    }
}
