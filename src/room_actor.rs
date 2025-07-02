// #region IMPORTS
use crate::{data_types::*, game_actor::*, pending_tracker::*, session_client_actor::*};
use kameo::{
    Actor,
    actor::{ActorID, ActorRef, WeakActorRef},
    error::{ActorStopReason, Infallible},
    message::{Context, Message},
};
use std::{
    collections::HashMap,
    ops::ControlFlow,
    time::{Duration, Instant},
};
use tracing::{error, warn};
use uuid::Uuid;
// #endregion

// #region ACTOR

#[derive(Copy, Clone, PartialEq)]
enum RoomPending {
    Question { uuid: Uuid },
    Round,
}

pub struct RoomActor {
    name: String,
    clients: HashMap<Uuid, RoomClient>,
    game_settings: GameSettings,
    game: WeakActorRef<GameActor>,

    current_question: Option<QuestionInfo>,
    current_answers: Vec<AnswerInfo>,

    is_game_running: bool,
    round_ticket: Option<Ticket<RoomPending>>,
    pending: PendingTracker<Self, RoomPending>,
    round_start: Option<Instant>,
    rounds_played: u64,
}

impl Actor for RoomActor {
    type Args = (String, WeakActorRef<GameActor>);
    type Error = Infallible;

    async fn on_start((name, game): Self::Args, ar: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(Self {
            name,
            clients: HashMap::new(),
            game_settings: GameSettings::default(),
            game,
            current_question: None,
            current_answers: Vec::new(),
            is_game_running: false,
            round_ticket: None,
            pending: PendingTracker::new(ar.downgrade()),
            round_start: None,
            rounds_played: 0,
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
    fn find_client(
        &self,
        session_id: ActorID,
    ) -> Option<(Uuid, RoomClientInfo, ActorRef<SessionClientActor>)> {
        self.clients
            .iter()
            .find(|(_, c)| c.session.id() == session_id)
            .map(|(&uuid, c)| (uuid, c.room_info, c.session.clone()))
    }

    async fn broadcast(&self, ws: TransportMsg) {
        for RoomClient { session, .. } in self.clients.values() {
            session.tell(SendWs(ws.clone())).await.ok();
        }
    }

    async fn reply_status(
        &self,
        session: &ActorRef<SessionClientActor>,
        correlation_id: Uuid,
        status: &str,
    ) {
        session
            .tell(SendWs(TransportMsg::OutRespStatus(TransportEnvelope {
                correlation_id,
                payload: OutRespStatus {
                    status: status.to_string(),
                },
            })))
            .await
            .ok();
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

    async fn finish_round(&mut self) {
        if !self.is_game_running {
            return;
        }

        self.push_missing_answers();

        let notif = TransportMsg::OutNotifRoundEnded(TransportEnvelope {
            correlation_id: Uuid::new_v4(),
            payload: OutNotifRoundEnded {
                question: self.current_question.clone().unwrap_or_default(),
                answers: self.current_answers.clone(),
            },
        });
        self.broadcast(notif).await;

        self.current_question = None;
        self.current_answers.clear();
        self.round_ticket = None;
        self.round_start = None;

        self.rounds_played += 1;

        if self.rounds_played >= self.game_settings.rounds_count {
            self.is_game_running = false;

            let stop_notif = TransportMsg::OutNotifGameStopped(TransportEnvelope {
                correlation_id: Uuid::new_v4(),
                payload: OutNotifGameStopped {
                    question: QuestionInfo::default(),
                    answers: Vec::new(),
                },
            });
            self.broadcast(stop_notif).await;
            return;
        }

        self.request_question().await;
    }

    async fn request_question(&mut self) {
        let Some((&admin_uuid, admin)) = self.clients.iter().find(|(_, c)| c.room_info.is_admin)
        else {
            warn!("no admin left – stopping game");
            self.is_game_running = false;
            return;
        };

        let corr_id = self.pending.add(
            RoomPending::Question { uuid: admin_uuid },
            Duration::from_secs(5),
        );

        let req = TransportMsg::OutReqQuestion(TransportEnvelope {
            correlation_id: corr_id.into(),
            payload: OutReqQuestion {},
        });
        admin.session.tell(SendWs(req)).await.ok();
    }

    fn push_missing_answers(&mut self) {
        let answered: std::collections::HashSet<String> =
            self.current_answers.iter().map(|a| a.id.clone()).collect();

        let max_time = self.game_settings.round_duration * 1_000;

        for uuid in self.clients.keys() {
            let id = uuid.to_string();
            if answered.contains(&id) {
                continue;
            }
            self.current_answers.push(AnswerInfo {
                id,
                answer: String::new(),
                is_correct: false,
                answer_time: max_time,
            });
        }
    }
}

// #endregion

// #region TYPES
#[derive(Debug, Clone, Copy)]
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
impl Message<Timeout> for RoomActor {
    type Reply = ();
    async fn handle(&mut self, Timeout(id): Timeout, _ctx: &mut Context<Self, ()>) {
        if let Some(meta) = self.pending.take(id.into()) {
            match meta.kind {
                RoomPending::Question { uuid } => {
                    warn!("admin {} didn't provide question in time", uuid);
                }
                RoomPending::Round => {
                    self.round_ticket = None;
                    self.finish_round().await;
                }
            }
        }
    }
}

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
        } else {
            let notif = TransportMsg::OutNotifGameSettingsChanged(TransportEnvelope {
                correlation_id: Uuid::new_v4(),
                payload: OutNotifGameSettingsChanged {
                    game_settings: self.game_settings.clone(),
                },
            });
            self.broadcast(notif).await;
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
        let Some((_, room_info, _)) = self.find_client(requester.id()) else {
            error!("no client");
            return;
        };

        if !room_info.is_admin {
            self.reply_status(&requester, correlation_id, "not admin")
                .await;
            return;
        }

        self.game_settings = game_settings.clone();
        self.reply_status(&requester, correlation_id, "success")
            .await;

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
        let Some((sender_uuid, _, _)) = self.find_client(requester.id()) else {
            error!("no client");
            return;
        };

        self.reply_status(&requester, correlation_id, "success")
            .await;

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

pub struct StartGameRequest {
    pub requester: ActorRef<SessionClientActor>,
    pub correlation_id: Uuid,
    pub game_settings: GameSettings,
}

impl Message<StartGameRequest> for RoomActor {
    type Reply = ();

    async fn handle(
        &mut self,
        StartGameRequest {
            requester,
            correlation_id,
            game_settings,
        }: StartGameRequest,
        _ctx: &mut Context<Self, ()>,
    ) {
        let Some((_admin_uuid, room_info, _admin_session)) = self.find_client(requester.id())
        else {
            error!("no client");
            return;
        };

        if !room_info.is_admin {
            self.reply_status(&requester, correlation_id, "not admin")
                .await;
            warn!("not admin");
            return;
        }

        if self.is_game_running {
            self.reply_status(&requester, correlation_id, "already running")
                .await;
            warn!("already running");
            return;
        }

        self.current_question = None;
        self.current_answers.clear();
        self.round_ticket = None;
        self.game_settings = game_settings.clone();
        self.is_game_running = true;
        self.rounds_played = 0;

        self.reply_status(&requester, correlation_id, "success")
            .await;

        let notif = TransportMsg::OutNotifGameStarted(TransportEnvelope {
            correlation_id: Uuid::new_v4(),
            payload: OutNotifGameStarted { game_settings },
        });
        self.broadcast(notif).await;

        self.request_question().await;
    }
}

pub struct ProvideQuestionResponse {
    pub requester: ActorRef<SessionClientActor>,
    pub correlation_id: Uuid,
    pub question_info: QuestionInfo,
    pub question_svg: String,
}

impl Message<ProvideQuestionResponse> for RoomActor {
    type Reply = ();

    async fn handle(&mut self, msg: ProvideQuestionResponse, _ctx: &mut Context<Self, ()>) {
        let ProvideQuestionResponse {
            requester,
            correlation_id,
            question_info,
            question_svg,
        } = msg;

        match self.pending.take(correlation_id.into()) {
            Some(PendingMeta {
                kind: RoomPending::Question { uuid },
                ..
            }) if requester.id() == self.clients[&uuid].session.id() => {
                self.current_question = Some(question_info.clone());
                self.current_answers.clear();

                let notif = TransportMsg::OutNotifQuestion(TransportEnvelope {
                    correlation_id: Uuid::new_v4(),
                    payload: OutNotifQuestion { question_svg },
                });
                self.broadcast(notif).await;

                self.round_start = Some(Instant::now());

                let ticket = self.pending.add(
                    RoomPending::Round,
                    Duration::from_secs(self.game_settings.round_duration),
                );
                self.round_ticket = Some(ticket);
            }

            _ => warn!("unexpected or late IN_RESP_question (id = {correlation_id})"),
        }
    }
}

#[derive(Debug)]
pub struct SendAnswerRequest {
    pub requester: ActorRef<SessionClientActor>,
    pub correlation_id: Uuid,
    pub answer: String,
}

impl Message<SendAnswerRequest> for RoomActor {
    type Reply = ();

    async fn handle(
        &mut self,
        SendAnswerRequest {
            requester,
            correlation_id,
            answer,
        }: SendAnswerRequest,
        _ctx: &mut Context<Self, ()>,
    ) {
        let Some((uuid, _, _)) = self.find_client(requester.id()) else {
            error!("no client");
            return;
        };

        if !self.is_game_running || self.current_question.is_none() {
            self.reply_status(&requester, correlation_id, "no active round")
                .await;
            warn!("no active round");
            return;
        }

        if self
            .current_answers
            .iter()
            .any(|a| a.id == uuid.to_string())
        {
            self.reply_status(&requester, correlation_id, "already answered")
                .await;
            warn!("already answered");
            return;
        }

        let elapsed = self
            .round_start
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);

        let is_correct = self
            .current_question
            .as_ref()
            .map(|question| {
                question
                    .word_info
                    .readings
                    .iter()
                    .any(|reading| reading.reading == answer)
            })
            .unwrap_or(false);

        self.current_answers.push(AnswerInfo {
            id: uuid.to_string(),
            answer: answer.clone(),
            is_correct,
            answer_time: elapsed,
        });

        self.reply_status(&requester, correlation_id, "success")
            .await;

        let notif = TransportMsg::OutNotifClientAnswered(TransportEnvelope {
            correlation_id: Uuid::new_v4(),
            payload: OutNotifClientAnswered {
                id: uuid.to_string(),
            },
        });
        self.broadcast(notif).await;

        if self.current_answers.len() == self.clients.len() {
            if let Some(ticket) = self.round_ticket.take() {
                self.pending.cancel(ticket);
            }
            self.finish_round().await;
        }
    }
}

pub struct StopGameRequest {
    pub requester: ActorRef<SessionClientActor>,
    pub correlation_id: Uuid,
}

impl Message<StopGameRequest> for RoomActor {
    type Reply = ();

    async fn handle(
        &mut self,
        StopGameRequest {
            requester,
            correlation_id,
        }: StopGameRequest,
        _ctx: &mut Context<Self, ()>,
    ) {
        let Some((_uuid, room_info, _)) = self.find_client(requester.id()) else {
            error!("no client");
            return;
        };

        if !room_info.is_admin {
            self.reply_status(&requester, correlation_id, "not admin")
                .await;
            return;
        }

        if !self.is_game_running {
            self.reply_status(&requester, correlation_id, "not running")
                .await;
            return;
        }

        if let Some(ticket) = self.round_ticket.take() {
            self.pending.cancel(ticket);
        }

        if self.current_question.is_some() {
            self.push_missing_answers();
        }

        self.reply_status(&requester, correlation_id, "success")
            .await;

        let notif = TransportMsg::OutNotifGameStopped(TransportEnvelope {
            correlation_id: Uuid::new_v4(),
            payload: OutNotifGameStopped {
                question: self.current_question.clone().unwrap_or_default(),
                answers: self.current_answers.clone(),
            },
        });
        self.broadcast(notif).await;

        self.is_game_running = false;
        self.current_question = None;
        self.current_answers.clear();
        self.round_start = None;
        self.round_ticket = None;
        self.rounds_played = 0;
    }
}

// #endregion
