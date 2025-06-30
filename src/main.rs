#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    kanjilab_server::call_launch_server("8080")?;
    tokio::signal::ctrl_c().await?;
    kanjilab_server::call_stop_server()?;

    Ok(())
}
