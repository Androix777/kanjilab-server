use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Message<T> {
    pub correlation_id: Uuid,
    pub payload: T,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "messageType")]
#[serde(rename_all = "camelCase")]
pub enum WsMessage {
    // #region IN_REQ
    #[serde(rename = "IN_REQ_sendPublicKey")]
    InReqSendPublicKey(Message<InReqSendPublicKey>),

    #[serde(rename = "IN_REQ_verifysignature")]
    InReqVerifySignature(Message<InReqVerifySignature>),

    #[serde(rename = "IN_REQ_registerClient")]
    InReqRegisterClient(Message<InReqRegisterClient>),

    #[serde(rename = "IN_REQ_sendChat")]
    InReqSendChat(Message<InReqSendChat>),

    #[serde(rename = "IN_REQ_makeAdmin")]
    InReqMakeAdmin(Message<InReqMakeAdmin>),

    #[serde(rename = "IN_REQ_clientList")]
    InReqClientList(Message<InReqClientList>),

    #[serde(rename = "IN_REQ_startGame")]
    InReqStartGame(Message<InReqStartGame>),

    #[serde(rename = "IN_REQ_stopGame")]
    InReqStopGame(Message<InReqStopGame>),

    #[serde(rename = "IN_REQ_sendAnswer")]
    InReqSendAnswer(Message<InReqSendAnswer>),

    #[serde(rename = "IN_REQ_sendGameSettings")]
    InReqSendGameSettings(Message<InReqSendGameSettings>),
    // #endregion

    // #region OUT_RESP
    #[serde(rename = "OUT_RESP_clientRegistered")]
    OutRespClientRegistered(Message<OutRespClientRegistered>),

    #[serde(rename = "OUT_RESP_status")]
    OutRespStatus(Message<OutRespStatus>),

    #[serde(rename = "OUT_RESP_clientList")]
    OutRespClientList(Message<OutRespClientList>),

    #[serde(rename = "OUT_RESP_signMessage")]
    OutRespSignMessage(Message<OutRespSignMessage>),
    // #endregion

    // #region OUT_REQ
    #[serde(rename = "OUT_REQ_question")]
    OutReqQuestion(Message<OutReqQuestion>),
    // #endregion

    // #region IN_RESP
    #[serde(rename = "IN_RESP_question")]
    InRespQuestion(Message<InRespQuestion>),
    // #endregion

    // #region OUT_NOTIF
    #[serde(rename = "OUT_NOTIF_clientRegistered")]
    OutNotifClientRegistered(Message<OutNotifClientRegistered>),

    #[serde(rename = "OUT_NOTIF_clientDisconnected")]
    OutNotifClientDisconnected(Message<OutNotifClientDisconnected>),

    #[serde(rename = "OUT_NOTIF_chatSent")]
    OutNotifChatSent(Message<OutNotifChatSent>),

    #[serde(rename = "OUT_NOTIF_adminMade")]
    OutNotifAdminMade(Message<OutNotifAdminMade>),

    #[serde(rename = "OUT_NOTIF_gameStarted")]
    OutNotifGameStarted(Message<OutNotifGameStarted>),

    #[serde(rename = "OUT_NOTIF_gameStopped")]
    OutNotifGameStopped(Message<OutNotifGameStopped>),

    #[serde(rename = "OUT_NOTIF_question")]
    OutNotifQuestion(Message<OutNotifQuestion>),

    #[serde(rename = "OUT_NOTIF_clientAnswered")]
    OutNotifClientAnswered(Message<OutNotifClientAnswered>),

    #[serde(rename = "OUT_NOTIF_roundEnded")]
    OutNotifRoundEnded(Message<OutNotifRoundEnded>),

    #[serde(rename = "OUT_NOTIF_gameSettingsChanged")]
    OutNotifGameSettingsChanged(Message<OutNotifGameSettingsChanged>),
    // #endregion
}

pub fn parse(text: &str) -> Result<WsMessage, serde_json::Error> {
    serde_json::from_str::<WsMessage>(text)
}

// #region OTHER
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub id: String,
    pub key: String,
    pub name: String,
    pub is_admin: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionInfo {
    pub word_info: WordInfo,
    pub font_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WordPartExample {
    pub word: String,
    pub frequency: Option<f64>,
    pub reading: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WordPartInfo {
    pub word_part: String,
    pub word_part_reading: String,
    pub examples: Vec<WordPartExample>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReadingWithParts {
    pub reading: String,
    pub parts: Vec<WordPartInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WordInfo {
    pub word: String,
    pub meanings: Vec<Vec<Vec<String>>>,
    pub readings: Vec<ReadingWithParts>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnswerInfo {
    pub id: String,
    pub answer: String,
    pub is_correct: bool,
    pub answer_time: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GameSettings {
    pub min_frequency: u64,
    pub max_frequency: u64,
    pub using_max_frequency: bool,
    pub round_duration: u64,
    pub rounds_count: u64,
    pub word_part: Option<String>,
    pub word_part_reading: Option<String>,
    pub fonts_count: u64,
    pub first_font_name: Option<String>,
}

// #endregion

// #region IN_REQ
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InReqSendPublicKey {
    pub key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InReqVerifySignature {
    pub signature: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InReqRegisterClient {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InReqSendChat {
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InReqMakeAdmin {
    pub admin_password: String,
    pub client_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InReqClientList {}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InReqStartGame {
    pub game_settings: GameSettings,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InReqStopGame {}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InReqSendAnswer {
    pub answer: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InReqSendGameSettings {
    pub game_settings: GameSettings,
}
// #endregion

// #region OUT_RESP
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutRespClientRegistered {
    pub id: String,
    pub game_settings: GameSettings,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutRespStatus {
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutRespClientList {
    pub clients: Vec<ClientInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutRespSignMessage {
    pub message: String,
}
// #endregion

// #region OUT_REQ
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutReqQuestion {}
// #endregion

// #region IN_RESP
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InRespQuestion {
    pub question: QuestionInfo,
    pub question_svg: String,
}
// #endregion

// #region OUT_NOTIF
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutNotifClientRegistered {
    pub client: ClientInfo,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutNotifClientDisconnected {
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutNotifChatSent {
    pub id: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutNotifAdminMade {
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutNotifGameStarted {
    pub game_settings: GameSettings,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutNotifGameStopped {
    pub question: QuestionInfo,
    pub answers: Vec<AnswerInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutNotifQuestion {
    pub question_svg: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutNotifClientAnswered {
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutNotifRoundEnded {
    pub question: QuestionInfo,
    pub answers: Vec<AnswerInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutNotifGameSettingsChanged {
    pub game_settings: GameSettings,
}
// #endregion
