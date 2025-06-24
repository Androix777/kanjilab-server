use futures_util::{SinkExt, stream::SplitSink};
use kameo::{
    Actor,
    message::{Context, Message, StreamMessage},
};
use tokio::net::TcpStream;
use tokio_tungstenite::{WebSocketStream, tungstenite::Message as WsMsg};
use tracing::{debug, error, info};

use crate::data_types::WsMessage;

pub type ParseResult = Result<WsMessage, String>;

type StreamItem = StreamMessage<ParseResult, (), ()>;

#[derive(Actor)]
pub struct ClientActor {
    write: futures_util::stream::SplitSink<WebSocketStream<TcpStream>, WsMsg>,
}

impl ClientActor {
    pub fn new(write: SplitSink<WebSocketStream<TcpStream>, WsMsg>) -> Self {
        Self { write }
    }
}

pub struct SendRaw(pub String);

impl Message<StreamItem> for ClientActor {
    type Reply = ();

    async fn handle(&mut self, msg: StreamItem, ctx: &mut Context<Self, Self::Reply>) {
        match msg {
            StreamMessage::Started(()) => {
                info!("WS-stream attached");
            }

            StreamMessage::Next(Ok(ws_msg)) => {
                if let Err(err) = ctx.actor_ref().tell(ws_msg).try_send() {
                    tracing::warn!("mailbox full, drop message: {}", err);
                }
            }

            StreamMessage::Next(Err(err)) => {
                error!("Bad incoming JSON: {}", err);
            }

            StreamMessage::Finished(()) => {
                info!("WS-stream finished");
                let _ = ctx.actor_ref().stop_gracefully().await;
            }
        }
    }
}

impl Message<SendRaw> for ClientActor {
    type Reply = ();

    async fn handle(&mut self, SendRaw(text): SendRaw, _ctx: &mut Context<Self, Self::Reply>) {
        debug!("Send to the client: {}", text);
        let _ = self.write.send(WsMsg::Text(text.into())).await;
    }
}

impl Message<WsMessage> for ClientActor {
    type Reply = ();

    async fn handle(&mut self, msg: WsMessage, _ctx: &mut Context<Self, Self::Reply>) {
        match msg {
            WsMessage::InReqSendPublicKey(env) => {
                tracing::info!("key = {}", env.payload.key);
            }
            WsMessage::InReqRegisterClient(env) => {
                tracing::info!("name = {}", env.payload.name);
            }
            _ => {}
        }
    }
}
