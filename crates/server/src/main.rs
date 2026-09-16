#[tokio::main]
async fn main() -> anyhow::Result<()> {
    aster_server::run().await
}
