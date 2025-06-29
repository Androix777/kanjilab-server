use kameo::Actor;
use kanjilab_server::{game_actor::{GameActor, NewClient}, tools::setup_tracing};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    setup_tracing();

    let game = GameActor::spawn(());

    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let game_clone = game.clone();

        tokio::spawn(async move {
            game_clone.tell(NewClient(stream)).await.ok();
        });
    }
}
