#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logisticos_compliance::bootstrap::run().await
}
