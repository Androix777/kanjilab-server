use std::{collections::HashMap, ops::ControlFlow};

use futures_util::{StreamExt, future};
use kameo::{
    Actor,
    actor::{ActorID, ActorRef, WeakActorRef},
    error::{ActorStopReason, Infallible},
    message::{Context, Message},
};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error as WsErr, Message as WsMsg},
};
use tracing::{error, info};

use crate::{
    session_client_actor::{self, ClientActor},
    websocket_client_actor::{ToTransport, WebSocketClientActor},
};

pub struct NewClient(pub TcpStream);

pub struct GameActor {
    clients: HashMap<ActorID, ActorRef<ClientActor>>,
}

impl Actor for GameActor {
    type Args = ();
    type Error = Infallible;

    async fn on_start(_: Self::Args, _ar: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(Self {
            clients: HashMap::new(),
        })
    }

    async fn on_link_died(
        &mut self,
        _ar: WeakActorRef<Self>,
        id: ActorID,
        reason: ActorStopReason,
    ) -> Result<ControlFlow<ActorStopReason>, Self::Error> {
        if self.clients.remove(&id).is_some() {
            tracing::info!("client {id:?} disconnected: {reason:?}");
            return Ok(ControlFlow::Continue(()));
        }

        tracing::warn!("non-client link died: {id:?}, reason: {reason:?}");

        match reason {
            ActorStopReason::Normal => {
                Ok(ControlFlow::Continue(()))
            }
            other => {
                Ok(ControlFlow::Break(other))
            }
        }
    }
}

impl Message<NewClient> for GameActor {
    type Reply = ();

    async fn handle(&mut self, NewClient(stream): NewClient, ctx: &mut Context<Self, Self::Reply>) {
        if let Err(e) = self.spawn_client(stream, ctx).await {
            error!("can't start client: {e}");
        }
    }
}

impl GameActor {
    async fn spawn_client(
        &mut self,
        stream: TcpStream,
        ctx: &mut Context<Self, ()>,
    ) -> Result<(), String> {
        let ws_stream = accept_async(stream).await.map_err(|e| e.to_string())?;
        let (write, read) = ws_stream.split();

        let game_ref = ctx.actor_ref();
        let session_ref = ClientActor::spawn_link(&game_ref, ClientActor::new()).await;

        let transport_ref = WebSocketClientActor::spawn_link(
            &session_ref,
            WebSocketClientActor::new(write, session_ref.clone()),
        )
        .await;

        let recipient = transport_ref.clone().recipient::<ToTransport>();
        session_ref
            .tell(session_client_actor::SetTransport(recipient))
            .await
            .ok();

        let raw_stream = read.filter_map(|r: Result<WsMsg, WsErr>| {
            future::ready(match r {
                Ok(WsMsg::Text(txt)) => Some(Ok(txt.to_string())),
                Ok(_) => None,
                Err(e) => Some(Err(e.to_string())),
            })
        });
        transport_ref.attach_stream(raw_stream, (), ());

        self.clients.insert(session_ref.id(), session_ref);

        info!("client connected (total = {})", self.clients.len());
        Ok(())
    }
}
