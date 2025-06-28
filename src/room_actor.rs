// #region IMPORTS
use crate::{game_actor::*, session_client_actor::*};
use kameo::{
    Actor,
    actor::{ActorID, ActorRef, WeakActorRef},
    error::{ActorStopReason, Infallible},
    message::{Context, Message},
};
use std::{collections::HashMap, ops::ControlFlow};
use tracing::{info, warn};
use uuid::Uuid;
// #endregion

// #region ACTOR
pub struct RoomActor {
    name: String,
    clients: HashMap<Uuid, RoomClient>,
    game: WeakActorRef<GameActor>,
}

impl Actor for RoomActor {
    type Args = (String, WeakActorRef<GameActor>);
    type Error = Infallible;

    async fn on_start((name, game): Self::Args, _ar: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(Self {
            name,
            clients: HashMap::new(),
            game,
        })
    }

    async fn on_link_died(
        &mut self,
        _ar: WeakActorRef<Self>,
        id: ActorID,
        reason: ActorStopReason,
    ) -> Result<ControlFlow<ActorStopReason>, Self::Error> {
        let leaver = self
            .clients
            .iter()
            .find(|(_, rc)| rc.session.id() == id)
            .map(|(uuid, _)| *uuid);

        if let Some(uuid) = leaver {
            self.clients.remove(&uuid);
            info!("client {uuid} left room \"{}\": {:?}", self.name, reason);
        } else {
            warn!("non-client link died in room: {id:?}");
        }
        Ok(ControlFlow::Continue(()))
    }
}
// #endregion

// #region TYPES
pub struct RoomClientInfo {
    pub is_admin: bool,
}

struct RoomClient {
    session: ActorRef<SessionClientActor>,
    room_info: RoomClientInfo,
}
// #endregion

// #region MESSAGES
pub struct AddClient {
    pub uuid: Uuid,
    pub session: ActorRef<SessionClientActor>,
}

impl Message<AddClient> for RoomActor {
    type Reply = ();

    async fn handle(
        &mut self,
        AddClient { uuid, session }: AddClient,
        _ctx: &mut Context<Self, ()>,
    ) {
        let is_admin = self.clients.is_empty();
        self.clients.insert(
            uuid,
            RoomClient {
                session,
                room_info: RoomClientInfo { is_admin },
            },
        );
    }
}

pub struct ClientListRequest {
    pub requester: ActorRef<SessionClientActor>,
    pub correlation_id: Uuid,
}

impl Message<ClientListRequest> for RoomActor {
    type Reply = ();

    async fn handle(
        &mut self,
        ClientListRequest {
            requester,
            correlation_id,
        }: ClientListRequest,
        _ctx: &mut Context<Self, ()>,
    ) {
        let ids: Vec<Uuid> = self.clients.keys().cloned().collect();

        let Some(game) = self.game.upgrade() else {
            return;
        };

        let game_infos = match game.ask(GetClientsInfo { ids: ids.clone() }).await {
            Ok(v) => v,
            Err(_) => return,
        };
        let mut by_id: HashMap<Uuid, GameClientInfo> =
            game_infos.into_iter().map(|g| (g.id, g)).collect();

        use crate::data_types::{
            ClientInfo, OutRespClientList, TransportEnvelope as Msg, TransportMsg,
        };
        let clients: Vec<ClientInfo> = ids
            .into_iter()
            .filter_map(|id| {
                let g = by_id.remove(&id)?;
                let r = &self.clients[&id].room_info;
                Some(ClientInfo {
                    id: g.id.to_string(),
                    key: g.key,
                    name: g.name,
                    is_admin: r.is_admin,
                })
            })
            .collect();

        let ws = TransportMsg::OutRespClientList(Msg {
            correlation_id,
            payload: OutRespClientList { clients },
        });
        requester.tell(SendWs(ws)).await.ok();
    }
}
// #endregion
