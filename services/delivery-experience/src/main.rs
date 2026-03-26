use logisticos_delivery_experience::bootstrap;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    bootstrap::run().await
}
