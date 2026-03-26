use logisticos_cdp::bootstrap;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    bootstrap::run().await
}
