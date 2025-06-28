// #region IMPORTS

use futures_util::{SinkExt, stream::SplitSink};
use kameo::{
    Actor,
    actor::WeakActorRef,
    message::{Context, Message, StreamMessage},
};
use tokio::net::TcpStream;
use tokio_tungstenite::{WebSocketStream, tungstenite::Message as WsMsg};
use tracing::error;

use crate::{data_types::*, session_client_actor::*};

pub type RawResult = Result<String, String>;
type StreamItem = StreamMessage<RawResult, (), ()>;

// #endregion

// #region ACTOR

#[derive(Actor)]
pub struct WebSocketClientActor {
    write: SplitSink<WebSocketStream<TcpStream>, WsMsg>,
    session: WeakActorRef<SessionClientActor>,
}

impl WebSocketClientActor {
    pub fn new(
        write: SplitSink<WebSocketStream<TcpStream>, WsMsg>,
        session: WeakActorRef<SessionClientActor>,
    ) -> Self {
        Self { write, session }
    }

    async fn send_to_session(&self, ws_msg: WsMessage) {
        if let Some(session) = self.session.upgrade() {
            let _ = session.tell(ws_msg).await;
        }
    }
}

// #endregion

// #region MESSAGES

impl Message<StreamItem> for WebSocketClientActor {
    type Reply = ();

    async fn handle(&mut self, msg: StreamItem, ctx: &mut Context<Self, ()>) {
        match msg {
            StreamMessage::Started(()) => {}

            StreamMessage::Next(Ok(text)) => match parse(&text) {
                Ok(ws_msg) => self.send_to_session(ws_msg).await,
                Err(e) => error!("bad incoming json: {e}"),
            },

            StreamMessage::Next(Err(e)) => {
                error!("WebSocket read error: {e}");
            }

            StreamMessage::Finished(()) => {
                if let Some(session) = self.session.upgrade() {
                    let _ = session.kill();
                } else {
                    let _ = ctx.actor_ref().kill();
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum ToTransport {
    Raw(String),
    Ws(WsMessage),
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

// #endregion
