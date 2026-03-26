#[tokio::main]
async fn main() -> anyhow::Result<()> {
    driver_ops::bootstrap::run().await
}
