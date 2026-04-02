#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logisticos_pod::bootstrap::run().await
}
