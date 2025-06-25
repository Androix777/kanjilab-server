use futures_util::{SinkExt, stream::SplitSink};
use kameo::{
    Actor,
    actor::ActorRef,
    message::{Context, Message, StreamMessage},
};
use tokio::net::TcpStream;
use tokio_tungstenite::{WebSocketStream, tungstenite::Message as WsMsg};
use tracing::{error, info};

use crate::client_actor::ClientActor;
use crate::data_types::{WsMessage, serialize};

pub type ParseResult = Result<WsMessage, String>;
type StreamItem = StreamMessage<ParseResult, (), ()>;

#[derive(Debug)]
pub enum ToTransport {
    Raw(String),
    Ws(WsMessage),
}

#[derive(Actor)]
pub struct WebSocketClientActor {
    write: SplitSink<WebSocketStream<TcpStream>, WsMsg>,
    session: ActorRef<ClientActor>,
}

impl WebSocketClientActor {
    pub fn new(
        write: SplitSink<WebSocketStream<TcpStream>, WsMsg>,
        session: ActorRef<ClientActor>,
    ) -> Self {
        Self { write, session }
    }
}

impl Message<StreamItem> for WebSocketClientActor {
    type Reply = ();

    async fn handle(&mut self, msg: StreamItem, _ctx: &mut Context<Self, Self::Reply>) {
        match msg {
            StreamMessage::Started(()) => {
                info!("WS stream attached");
            }
            StreamMessage::Next(Ok(ws_msg)) => {
                let _ = self.session.tell(ws_msg).try_send();
            }
            StreamMessage::Next(Err(e)) => {
                error!("bad incoming json: {e}");
            }
            StreamMessage::Finished(()) => {
                info!("WS stream finished");
                let _ = _ctx.actor_ref().kill();
            }
        }
    }
}

impl Message<ToTransport> for WebSocketClientActor {
    type Reply = ();

    async fn handle(&mut self, msg: ToTransport, _ctx: &mut Context<Self, Self::Reply>) {
        match msg {
            ToTransport::Raw(text) => {
                let _ = self.write.send(WsMsg::Text(text.into())).await;
            }
            ToTransport::Ws(ws_msg) => match serialize(&ws_msg) {
                Ok(text) => {
                    let _ = self.write.send(WsMsg::Text(text.into())).await;
                }
                Err(e) => error!("serialize error: {e}"),
            },
        }
    }
}
