#[tokio::main]
async fn main() -> anyhow::Result<()> {
    payments::bootstrap::run().await
}
