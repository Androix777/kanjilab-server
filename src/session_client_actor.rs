// #region IMPORTS
use crate::{data_types::*, game_actor::*, room_actor::*, tools::*, websocket_client_actor::*};
use kameo::{
    Actor,
    actor::{Recipient, WeakActorRef},
    message::{Context, Message},
};
use tracing::{debug, error, warn};
use uuid::Uuid;
// #endregion

// #region ACTOR
#[derive(Actor)]
pub struct SessionClientActor {
    transport: Option<Recipient<ToTransport>>,

    pub_key: Option<String>,
    sign_challenge: Option<Uuid>,
    signature_verified: bool,

    game: WeakActorRef<GameActor>,
    room: Option<WeakActorRef<RoomActor>>,
}

impl SessionClientActor {
    pub fn new(game: WeakActorRef<GameActor>) -> Self {
        Self {
            transport: None,
            pub_key: None,
            sign_challenge: None,
            signature_verified: false,
            game,
            room: None,
        }
    }

    async fn send(&self, msg: ToTransport) {
        if let Some(tx) = &self.transport {
            tx.tell(msg).await.ok();
        }
    }

    pub async fn send_transport(&self, ws: TransportMsg) {
        self.send(ToTransport::TransportMsg(ws)).await;
    }

    async fn send_status<P>(&self, env: &TransportEnvelope<P>, status: &str) {
        let ws = TransportMsg::OutRespStatus(TransportEnvelope {
            correlation_id: env.correlation_id,
            payload: OutRespStatus {
                status: status.to_string(),
            },
        });

        self.send(ToTransport::TransportMsg(ws)).await;
    }

    fn current_challenge_str(&self) -> Option<String> {
        self.sign_challenge.map(|u| u.to_string())
    }
}
// #endregion

// #region MESSAGES
pub struct Shutdown;

impl Message<Shutdown> for SessionClientActor {
    type Reply = ();

    async fn handle(&mut self, _: Shutdown, ctx: &mut Context<Self, ()>) {
        debug!("Client kicked");
        ctx.actor_ref().kill();
    }
}

pub struct SetTransport(pub Recipient<ToTransport>);
impl Message<SetTransport> for SessionClientActor {
    type Reply = ();
    async fn handle(&mut self, SetTransport(rec): SetTransport, _ctx: &mut Context<Self, ()>) {
        self.transport = Some(rec);
    }
}

pub struct SendWs(pub TransportMsg);
impl Message<SendWs> for SessionClientActor {
    type Reply = ();
    async fn handle(&mut self, SendWs(ws): SendWs, _ctx: &mut Context<Self, Self::Reply>) {
        self.send(ToTransport::TransportMsg(ws)).await;
    }
}

impl Message<TransportMsg> for SessionClientActor {
    type Reply = ();

    async fn handle(&mut self, msg: TransportMsg, ctx: &mut Context<Self, Self::Reply>) {
        match msg {
            TransportMsg::InReqSendPublicKey(env) => {
                debug!("IN_REQ_sendPublicKey {}", env.payload.key);

                if self.signature_verified {
                    warn!("signature already verified");
                    self.send_status(&env, "signature already verified").await;
                }

                self.pub_key = Some(env.payload.key.clone());

                let challenge = Uuid::new_v4();
                self.sign_challenge = Some(challenge);

                let resp = TransportMsg::OutRespSignMessage(TransportEnvelope {
                    correlation_id: env.correlation_id,
                    payload: OutRespSignMessage {
                        message: challenge.to_string(),
                    },
                });
                self.send(ToTransport::TransportMsg(resp)).await;
            }

            TransportMsg::InReqVerifySignature(env) => {
                debug!("IN_REQ_verifySignature {}", env.payload.signature);

                let Some(challenge) = self.current_challenge_str() else {
                    warn!("no stored challenge");
                    self.send_status(&env, "no stored challenges").await;
                    return;
                };
                let Some(key) = self.pub_key.clone() else {
                    warn!("no public key");
                    self.send_status(&env, "no public key").await;
                    return;
                };

                let is_ok = match verify_signature(&challenge, &env.payload.signature, &key) {
                    Ok(ok) => ok,
                    Err(e) => {
                        warn!("verify_signature error: {e}");
                        self.send_status(&env, "error").await;
                        false
                    }
                };

                self.signature_verified = is_ok;

                let status_text = if is_ok { "success" } else { "error" }.to_string();
                let resp = TransportMsg::OutRespStatus(TransportEnvelope {
                    correlation_id: env.correlation_id,
                    payload: OutRespStatus {
                        status: status_text,
                    },
                });
                self.send(ToTransport::TransportMsg(resp)).await;
            }

            TransportMsg::InReqRegisterClient(env) => {
                debug!("IN_REQ_registerClient {}", env.payload.name);
                if !self.signature_verified {
                    warn!("register requested before signature verified");
                    self.send_status(&env, "error").await;
                    return;
                }

                let Some(game) = self.game.upgrade() else {
                    warn!("game actor gone");
                    self.send_status(&env, "error").await;
                    return;
                };
                let Some(key) = self.pub_key.clone() else {
                    warn!("no public key");
                    self.send_status(&env, "no public key").await;
                    return;
                };

                let req = RegisterClientRequest {
                    session: ctx.actor_ref().clone(),
                    name: env.payload.name.clone(),
                    pub_key: key,
                    correlation_id: env.correlation_id,
                };
                game.tell(req).await.ok();
            }

            TransportMsg::InReqClientList(env) => {
                debug!("IN_REQ_clientList");
                if let Some(room) = self.room.as_ref().and_then(|r| r.upgrade()) {
                    room.tell(ClientListRequest {
                        requester: ctx.actor_ref().clone(),
                        correlation_id: env.correlation_id,
                    })
                    .await
                    .ok();
                } else {
                    warn!("client asked for clientList but has no room");
                    self.send_status(&env, "no room").await;
                }
            }

            TransportMsg::InReqSendGameSettings(env) => {
                debug!("IN_REQ_sendGameSettings");
                if let Some(room) = self.room.as_ref().and_then(|r| r.upgrade()) {
                    room.tell(SetGameSettingsRequest {
                        requester: ctx.actor_ref().clone(),
                        correlation_id: env.correlation_id,
                        game_settings: env.payload.game_settings.clone(),
                    })
                    .await
                    .ok();
                } else {
                    warn!("client asked to change settings but has no room");
                    self.send_status(&env, "no room").await;
                }
            }

            TransportMsg::InReqSendChat(env) => {
                debug!("IN_REQ_sendChat");
                if let Some(room) = self.room.as_ref().and_then(|r| r.upgrade()) {
                    room.tell(SendChatRequest {
                        requester: ctx.actor_ref().clone(),
                        correlation_id: env.correlation_id,
                        message: env.payload.message.clone(),
                    })
                    .await
                    .ok();
                } else {
                    warn!("client asked to sendChat but has no room");
                    self.send_status(&env, "no room").await;
                }
            }

            TransportMsg::InReqStartGame(env) => {
                debug!("IN_REQ_startGame");
                if let Some(room) = self.room.as_ref().and_then(|r| r.upgrade()) {
                    room.tell(StartGameRequest {
                        requester: ctx.actor_ref().clone(),
                        correlation_id: env.correlation_id,
                        game_settings: env.payload.game_settings.clone(),
                    })
                    .await
                    .ok();
                } else {
                    self.send_status(&env, "no room").await;
                }
            }

            TransportMsg::InRespQuestion(env) => {
                debug!("IN_RESP_question");
                if let Some(room) = self.room.as_ref().and_then(|r| r.upgrade()) {
                    room.tell(ProvideQuestionResponse {
                        requester: ctx.actor_ref().clone(),
                        correlation_id: env.correlation_id,
                        question_info: env.payload.question.clone(),
                        question_svg: env.payload.question_svg.clone(),
                    })
                    .await
                    .ok();
                }
            }

            TransportMsg::InReqSendAnswer(env) => {
                debug!("IN_REQ_sendAnswer");
                if let Some(room) = self.room.as_ref().and_then(|r| r.upgrade()) {
                    room.tell(SendAnswerRequest {
                        requester: ctx.actor_ref().clone(),
                        correlation_id: env.correlation_id,
                        answer: env.payload.answer.clone(),
                    })
                    .await
                    .ok();
                } else {
                    self.send_status(&env, "no room").await;
                }
            }

            TransportMsg::InReqStopGame(env) => {
                if let Some(room) = self.room.as_ref().and_then(|r| r.upgrade()) {
                    room.tell(StopGameRequest {
                        requester: ctx.actor_ref().clone(),
                        correlation_id: env.correlation_id,
                    })
                    .await
                    .ok();
                } else {
                    self.send_status(&env, "no room").await;
                }
            }

            _ => error!("Unknown message: {msg:?}"),
        }
    }
}

pub struct SendRaw(pub String);
impl Message<SendRaw> for SessionClientActor {
    type Reply = ();
    async fn handle(&mut self, SendRaw(text): SendRaw, _ctx: &mut Context<Self, Self::Reply>) {
        self.send(ToTransport::Raw(text)).await;
    }
}

pub struct SetRoom(pub WeakActorRef<RoomActor>);
impl Message<SetRoom> for SessionClientActor {
    type Reply = ();
    async fn handle(&mut self, SetRoom(room): SetRoom, _ctx: &mut Context<Self, ()>) {
        self.room = Some(room);
    }
}
// #endregion
