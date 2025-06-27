use std::{collections::HashMap, ops::ControlFlow};

use kameo::{
    actor::{ActorID, ActorRef, WeakActorRef},
    error::{ActorStopReason, Infallible},
    message::{Context, Message},
    Actor,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::session_client_actor::SessionClientActor;


pub struct RoomActor {
    name: String,
    clients: HashMap<Uuid, ActorRef<SessionClientActor>>,
}

impl Actor for RoomActor {
    type Args = String;
    type Error = Infallible;

    async fn on_start(name: String, _ar: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(Self {
            name,
            clients: HashMap::new(),
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
            .find(|(_, s)| s.id() == id)
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

pub struct AddClient {
    pub uuid: Uuid,
    pub session: ActorRef<SessionClientActor>,
}

impl Message<AddClient> for RoomActor {
    type Reply = ();

    async fn handle(&mut self, AddClient { uuid, session }: AddClient, _ctx: &mut Context<Self, ()>) {
        self.clients.insert(uuid, session);
        info!("client {uuid} joined room \"{}\"", self.name);
    }
}
