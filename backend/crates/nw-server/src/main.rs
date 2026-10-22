#[tokio::main]
async fn main() -> anyhow::Result<()> {
    nw_server::run().await
}
