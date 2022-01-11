// use tracing::{info, Level};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use kustd::Manager;

#[tokio::main(flavor="multi_thread")]
async fn main() -> kustd::Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    let manager = Manager::new().await;
    manager.start().await;

    Ok(())
}
