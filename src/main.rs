use futures_util::{StreamExt, future};
use kameo::Actor;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::{Error as WsErr, Message as WsMsg};
use tracing::Level;
use tracing_subscriber::{self, fmt::time::LocalTime};

mod client_actor;
mod data_types;
use client_actor::{ClientActor, SendRaw};

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
    let actor = ClientActor::spawn(ClientActor::new(write));

    let parsed = read.filter_map(|r: Result<WsMsg, WsErr>| {
        future::ready(match r {
            Ok(WsMsg::Text(t)) => Some(data_types::parse(&t).map_err(|e| e.to_string())),
            Ok(_) => None,
            Err(e) => Some(Err(e.to_string())),
        })
    });

    actor.attach_stream(parsed, (), ());

    tokio::spawn({
        let actor = actor.clone();
        async move {
            let mut n = 0usize;
            loop {
                if actor
                    .tell(SendRaw(format!("\"tick {}\"", n)))
                    .await
                    .is_err()
                {
                    break;
                }
                n += 1;
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    });
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
