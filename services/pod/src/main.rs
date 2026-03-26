#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pod::bootstrap::run().await
}
