use serde_json;
use ws;

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Inbound {
    Logout,
    Login { username: String, password: String },
    Join { channel: String },
    Message { channel: String, text: String },
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Outbound {
    Success,
    AuthFail,
    NotAuthed,
    NotInChannel,
    Joined {
        channel: String,
        username: String,
    },
    Message {
        channel: String,
        username: String,
        text: String,
    },
    FormatError {
        error: String,
    },
}

impl Into<ws::Message> for Outbound {
    fn into(self) -> ws::Message {
        ws::Message::Text(serde_json::to_string(&self).unwrap())
    }
}
