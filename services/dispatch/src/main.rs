#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dispatch::bootstrap::run().await
}
