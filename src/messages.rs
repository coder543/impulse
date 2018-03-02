use serde_json;
use ws;

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Inbound {
    Logout,
    Login { username: String, password: String },
    Join { channel: String },
    Leave { channel: String },
    ChannelInfo { channel: String },
    JoinedChannels,
    AllChannels,
    Message { channel: String, text: String },
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Outbound<'a> {
    Success,
    AuthFail,
    NotAuthed,
    NotInChannel,
    NoSuchChannel,
    Joined {
        channel: &'a str,
        username: &'a str,
    },
    Left {
        channel: &'a str,
        username: &'a str,
    },
    ChannelInfo {
        members: Vec<&'a str>,
    },
    Channels {
        channels: Vec<&'a str>,
    },
    Message {
        channel: &'a str,
        username: &'a str,
        text: &'a str,
    },
    FormatError {
        error: &'a str,
    },
}

impl<'a> Into<ws::Message> for Outbound<'a> {
    fn into(self) -> ws::Message {
        ws::Message::Text(serde_json::to_string(&self).unwrap())
    }
}
