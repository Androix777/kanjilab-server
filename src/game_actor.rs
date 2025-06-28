// #region IMPORTS
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
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    data_types::{self, *},
    room_actor::*,
    session_client_actor::{self, *},
    websocket_client_actor::*,
};
// #endregion

// #region ACTOR
pub struct GameActor {
    pending_clients: HashMap<Uuid, ActorRef<SessionClientActor>>,
    registered_clients: HashMap<Uuid, RegisteredClient>,
    room: ActorRef<RoomActor>,
}

impl Actor for GameActor {
    type Args = ();
    type Error = Infallible;

    async fn on_start(_: Self::Args, ar: ActorRef<Self>) -> Result<Self, Self::Error> {
        let room = RoomActor::spawn_link(&ar, ("default".into(), ar.downgrade())).await;

        Ok(Self {
            pending_clients: HashMap::new(),
            registered_clients: HashMap::new(),
            room,
        })
    }

    async fn on_link_died(
        &mut self,
        _ar: WeakActorRef<Self>,
        id: ActorID,
        reason: ActorStopReason,
    ) -> Result<ControlFlow<ActorStopReason>, Self::Error> {
        let mut uuid_to_remove: Option<Uuid> = None;
        for (uuid, session) in &self.pending_clients {
            if session.id() == id {
                uuid_to_remove = Some(*uuid);
                break;
            }
        }
        if let Some(uuid) = uuid_to_remove {
            self.pending_clients.remove(&uuid);
            info!("pending client {uuid} disconnected: {reason:?}");
            return Ok(ControlFlow::Continue(()));
        }

        uuid_to_remove = None;
        for (uuid, client) in &self.registered_clients {
            if client.session.id() == id {
                uuid_to_remove = Some(*uuid);
                break;
            }
        }
        if let Some(uuid) = uuid_to_remove {
            self.registered_clients.remove(&uuid);
            info!("registered client {uuid} disconnected: {reason:?}");
            return Ok(ControlFlow::Continue(()));
        }

        warn!("non-client link died: {id:?}, reason: {reason:?}");
        match reason {
            ActorStopReason::Normal => Ok(ControlFlow::Continue(())),
            other => Ok(ControlFlow::Break(other)),
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
        let session_ref = SessionClientActor::spawn_link(
            &game_ref,
            SessionClientActor::new(game_ref.downgrade()),
        )
        .await;

        let transport_ref = WebSocketClientActor::spawn_link(
            &session_ref,
            WebSocketClientActor::new(write, session_ref.downgrade()),
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

        let client_uuid = Uuid::new_v4();
        self.pending_clients.insert(client_uuid, session_ref);

        info!(
            "client connected (total = {})",
            self.pending_clients.len() + self.registered_clients.len()
        );
        Ok(())
    }
}
// #endregion

// #region TYPES

#[derive(Clone)]
pub struct GameClientInfo {
    pub id: Uuid,
    pub key: String,
    pub name: String,
}

struct RegisteredClient {
    session: ActorRef<SessionClientActor>,
    info: GameClientInfo,
}
// #endregion

// #region MESSAGES
pub struct NewClient(pub TcpStream);

impl Message<NewClient> for GameActor {
    type Reply = ();

    async fn handle(&mut self, NewClient(stream): NewClient, ctx: &mut Context<Self, Self::Reply>) {
        if let Err(e) = self.spawn_client(stream, ctx).await {
            error!("can't start client: {e}");
        }
    }
}

pub struct RegisterClientRequest {
    pub session: ActorRef<SessionClientActor>,
    pub name: String,
    pub pub_key: String,
    pub correlation_id: Uuid,
}

impl Message<RegisterClientRequest> for GameActor {
    type Reply = ();

    async fn handle(&mut self, msg: RegisterClientRequest, _ctx: &mut Context<Self, ()>) {
        let RegisterClientRequest {
            session,
            name,
            pub_key,
            correlation_id,
        } = msg;

        let client_uuid = self
            .pending_clients
            .iter()
            .find(|(_, s)| s.id() == session.id())
            .map(|(uuid, _)| *uuid);

        let Some(uuid) = client_uuid else {
            warn!("registration for unknown session actor: {:?}", session.id());
            return;
        };

        let session_ref = self.pending_clients.remove(&uuid).unwrap();
        self.registered_clients.insert(
            uuid,
            RegisteredClient {
                session: session_ref.clone(),
                info: GameClientInfo {
                    id: uuid,
                    key: pub_key.clone(),
                    name: name.clone(),
                },
            },
        );

        let _ = self
            .room
            .tell(AddClient {
                uuid,
                session: session_ref.clone(),
            })
            .await;

        session_ref
            .tell(session_client_actor::SetRoom(self.room.downgrade()))
            .await
            .ok();

        let resp = WsMessage::OutRespClientRegistered(data_types::Message {
            correlation_id,
            payload: OutRespClientRegistered {
                id: uuid.to_string(),
                game_settings: GameSettings::default(),
            },
        });
        session_ref.tell(SendWs(resp)).await.ok();

        info!("client \"{name}\" registered as {uuid}");
    }
}

pub struct GetClientsInfo {
    pub ids: Vec<Uuid>,
}

impl Message<GetClientsInfo> for GameActor {
    type Reply = Vec<GameClientInfo>;

    async fn handle(
        &mut self,
        GetClientsInfo { ids }: GetClientsInfo,
        _ctx: &mut Context<Self, Self::Reply>,
    ) -> Vec<GameClientInfo> {
        ids.into_iter()
            .filter_map(|id| self.registered_clients.get(&id).map(|c| c.info.clone()))
            .collect()
    }
}
// #endregion
