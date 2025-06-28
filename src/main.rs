use kameo::Actor;
use kanjilab_server::game_actor::{GameActor, NewClient};
use tokio::net::TcpListener;
use tracing::Level;
use tracing_subscriber::fmt::time::LocalTime;

#[tokio::main]
async fn main() {
    setup_tracing();

    let game = GameActor::spawn(());

    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let game_clone = game.clone();

        tokio::spawn(async move {
            let _ = game_clone.tell(NewClient(stream)).await;
        });
    }
}

fn setup_tracing() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .with_target(false)
        .with_timer(LocalTime::new(time::macros::format_description!(
            "[hour]:[minute]:[second].[subsecond digits:3]"
        )))
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set global logger");
}
