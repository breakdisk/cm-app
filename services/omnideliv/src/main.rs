#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logisticos_omnideliv::bootstrap::run().await
}
