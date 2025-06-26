use futures_util::{StreamExt, future};
use kameo::Actor;
use kanjilab_server::websocket_client_actor::ToTransport;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{tungstenite::Error as WsErr, tungstenite::Message as WsMsg};
use tracing::Level;
use tracing_subscriber::{self, fmt::time::LocalTime};

use kanjilab_server::session_client_actor::{self, ClientActor};
use kanjilab_server::websocket_client_actor::WebSocketClientActor;

#[tokio::main]
async fn main() {
    setup_tracing();
    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        tokio::spawn(handle_tcp(stream));
    }
}

async fn handle_tcp(stream: TcpStream) {
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("handshake");

    let (write, read) = ws_stream.split();

    let session_ref = ClientActor::spawn(ClientActor::new());

    let transport_ref = WebSocketClientActor::spawn_link(
        &session_ref,
        WebSocketClientActor::new(write, session_ref.clone()),
    )
    .await;

    let transport_recipient = transport_ref.clone().recipient::<ToTransport>();

    session_ref
        .tell(session_client_actor::SetTransport(transport_recipient))
        .await
        .ok();

    let raw_stream = read.filter_map(|r: Result<WsMsg, WsErr>| {
        future::ready(match r {
            Ok(WsMsg::Text(t)) => Some(Ok(t.to_string())),
            Ok(_) => None,
            Err(e) => Some(Err(e.to_string())),
        })
    });
    transport_ref.attach_stream(raw_stream, (), ());
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
