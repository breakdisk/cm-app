#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logisticos_field_ops::bootstrap::run().await
}
