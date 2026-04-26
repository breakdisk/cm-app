use logisticos_webhooks::bootstrap;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    bootstrap::run().await
}
