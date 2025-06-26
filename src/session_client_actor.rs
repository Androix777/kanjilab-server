use kameo::{
    Actor,
    actor::Recipient,
    message::{Context, Message},
};
use tracing::info;

use crate::data_types::*;
use crate::websocket_client_actor::ToTransport;

pub struct SendRaw(pub String);
pub struct SetTransport(pub Recipient<ToTransport>);

#[derive(Actor)]
pub struct ClientActor {
    transport: Option<Recipient<ToTransport>>,
}

impl ClientActor {
    pub fn new() -> Self {
        Self { transport: None }
    }

    async fn send_to_transport(&self, msg: ToTransport) {
        if let Some(tx) = &self.transport {
            let _ = tx.tell(msg).await;
        }
    }
}

impl Message<SetTransport> for ClientActor {
    type Reply = ();
    async fn handle(&mut self, SetTransport(rec): SetTransport, _ctx: &mut Context<Self, ()>) {
        self.transport = Some(rec);
    }
}

impl Message<WsMessage> for ClientActor {
    type Reply = ();
    async fn handle(&mut self, msg: WsMessage, _ctx: &mut Context<Self, Self::Reply>) {
        match msg {
            WsMessage::InReqSendPublicKey(env) => {
                info!("public key = {}", env.payload.key);
            }
            WsMessage::InReqRegisterClient(env) => {
                info!("register name = {}", env.payload.name);

                let resp = WsMessage::OutRespStatus(crate::data_types::Message {
                    correlation_id: env.correlation_id,
                    payload: OutRespStatus {
                        status: "ok".to_string(),
                    },
                });
                self.send_to_transport(ToTransport::Ws(resp)).await;
            }
            _ => {}
        }
    }
}

impl Message<SendRaw> for ClientActor {
    type Reply = ();
    async fn handle(&mut self, SendRaw(text): SendRaw, _ctx: &mut Context<Self, Self::Reply>) {
        self.send_to_transport(ToTransport::Raw(text)).await;
    }
}
