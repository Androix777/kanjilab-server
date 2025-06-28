// #region IMPORTS
use crate::{data_types::*, game_actor::*, session_client_actor::*};
use kameo::{
    Actor,
    actor::{ActorID, ActorRef, WeakActorRef},
    error::{ActorStopReason, Infallible},
    message::{Context, Message},
};
use std::{collections::HashMap, ops::ControlFlow};
use tracing::{debug, error};
use uuid::Uuid;
// #endregion

// #region ACTOR
pub struct RoomActor {
    name: String,
    clients: HashMap<Uuid, RoomClient>,
    game_settings: GameSettings,
    game: WeakActorRef<GameActor>,
}

impl Actor for RoomActor {
    type Args = (String, WeakActorRef<GameActor>);
    type Error = Infallible;

    async fn on_start((name, game): Self::Args, _ar: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(Self {
            name,
            clients: HashMap::new(),
            game_settings: GameSettings::default(),
            game,
        })
    }

    async fn on_link_died(
        &mut self,
        _ar: WeakActorRef<Self>,
        id: ActorID,
        _reason: ActorStopReason,
    ) -> Result<ControlFlow<ActorStopReason>, Self::Error> {
        if let Some(uuid) = self
            .clients
            .iter()
            .find(|(_, c)| c.session.id() == id)
            .map(|(u, _)| *u)
        {
            self.clients.remove(&uuid);
            self.notif_client_disconnected(uuid).await;

            if let Some((&new_admin_uuid, _)) = self.clients.iter().next() {
                if !self.clients[&new_admin_uuid].room_info.is_admin {
                    self.clients
                        .get_mut(&new_admin_uuid)
                        .unwrap()
                        .room_info
                        .is_admin = true;
                    self.notif_admin_made(new_admin_uuid).await;
                }
            }
        }
        Ok(ControlFlow::Continue(()))
    }
}

impl RoomActor {
    async fn broadcast(&self, ws: TransportMsg) {
        for RoomClient { session, .. } in self.clients.values() {
            session.tell(SendWs(ws.clone())).await.ok();
        }
    }

    async fn notif_client_registered(&self, client_info: ClientInfo) {
        let ws = TransportMsg::OutNotifClientRegistered(TransportEnvelope {
            correlation_id: Uuid::new_v4(),
            payload: OutNotifClientRegistered {
                client: client_info,
            },
        });
        self.broadcast(ws).await;
    }

    async fn notif_client_disconnected(&self, uuid: Uuid) {
        let ws = TransportMsg::OutNotifClientDisconnected(TransportEnvelope {
            correlation_id: Uuid::new_v4(),
            payload: OutNotifClientDisconnected {
                id: uuid.to_string(),
            },
        });
        self.broadcast(ws).await;
    }
    async fn notif_admin_made(&self, uuid: Uuid) {
        let ws = TransportMsg::OutNotifAdminMade(TransportEnvelope {
            correlation_id: Uuid::new_v4(),
            payload: OutNotifAdminMade {
                id: uuid.to_string(),
            },
        });
        self.broadcast(ws).await;
    }
}

// #endregion

// #region TYPES
#[derive(Debug)]
pub struct RoomClientInfo {
    pub is_admin: bool,
}

#[derive(Debug)]
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
        ctx: &mut Context<Self, ()>,
    ) {
        let is_admin = self.clients.is_empty();

        session.link(&ctx.actor_ref()).await;

        self.clients.insert(
            uuid,
            RoomClient {
                session: session.clone(),
                room_info: RoomClientInfo { is_admin },
            },
        );

        let Some(game) = self.game.upgrade() else {
            return;
        };
        let Ok(mut infos) = game.ask(GetClientsInfo { ids: vec![uuid] }).await else {
            return;
        };
        let Some(g) = infos.pop() else { return };

        let client_info = ClientInfo {
            id: g.id.to_string(),
            key: g.key,
            name: g.name,
            is_admin,
        };

        self.notif_client_registered(client_info).await;
        if is_admin {
            self.notif_admin_made(uuid).await;
        }
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

pub struct SetGameSettingsRequest {
    pub requester: ActorRef<SessionClientActor>,
    pub correlation_id: Uuid,
    pub game_settings: GameSettings,
}

impl Message<SetGameSettingsRequest> for RoomActor {
    type Reply = ();

    async fn handle(
        &mut self,
        SetGameSettingsRequest {
            requester,
            correlation_id,
            game_settings,
        }: SetGameSettingsRequest,
        _ctx: &mut Context<Self, ()>,
    ) {
        let Some((&_uuid, client)) = self
            .clients
            .iter()
            .find(|(_, c)| c.session.id() == requester.id())
        else {
            error!("no client");
            return;
        };

        if !client.room_info.is_admin {
            requester
                .tell(SendWs(TransportMsg::OutRespStatus(TransportEnvelope {
                    correlation_id,
                    payload: OutRespStatus {
                        status: "not admin".into(),
                    },
                })))
                .await
                .ok();
            return;
        }

        debug!("{game_settings:?}");

        self.game_settings = game_settings.clone();

        requester
            .tell(SendWs(TransportMsg::OutRespStatus(TransportEnvelope {
                correlation_id,
                payload: OutRespStatus {
                    status: "success".into(),
                },
            })))
            .await
            .ok();

        let notif = TransportMsg::OutNotifGameSettingsChanged(TransportEnvelope {
            correlation_id: Uuid::new_v4(),
            payload: OutNotifGameSettingsChanged { game_settings },
        });
        self.broadcast(notif).await;
    }
}

pub struct SendChatRequest {
    pub requester: ActorRef<SessionClientActor>,
    pub correlation_id: Uuid,
    pub message: String,
}

impl Message<SendChatRequest> for RoomActor {
    type Reply = ();

    async fn handle(
        &mut self,
        SendChatRequest {
            requester,
            correlation_id,
            message,
        }: SendChatRequest,
        _ctx: &mut Context<Self, ()>,
    ) {
        let Some((&sender_uuid, _)) = self
            .clients
            .iter()
            .find(|(_, c)| c.session.id() == requester.id())
        else {
            error!("no client");
            return;
        };

        requester
            .tell(SendWs(TransportMsg::OutRespStatus(TransportEnvelope {
                correlation_id,
                payload: OutRespStatus {
                    status: "success".into(),
                },
            })))
            .await
            .ok();

        let notif = TransportMsg::OutNotifChatSent(TransportEnvelope {
            correlation_id: Uuid::new_v4(),
            payload: OutNotifChatSent {
                id: sender_uuid.to_string(),
                message: message.clone(),
            },
        });
        self.broadcast(notif).await;
    }
}

// #endregion
