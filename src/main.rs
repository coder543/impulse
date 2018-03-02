#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

extern crate env_logger;
extern crate serde;
extern crate serde_json;
extern crate ws;

use ws::{listen, Handler, Sender};

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

lazy_static! {
    static ref SOCKETS: RwLock<HashMap<String, Sender>> =  RwLock::new(HashMap::new());
    static ref USERS: RwLock<HashMap<String, String>> = RwLock::new(HashMap::new());
    static ref CHANNELS: RwLock<HashMap<String, HashSet<String>>> = RwLock::new(HashMap::new());
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
enum Inbound {
    Logout,
    Login { username: String, password: String },
    Join { channel: String },
    Message { channel: String, text: String },
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type")]
enum Outbound {
    Success,
    AuthFail,
    NotAuthed,
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

struct Session {
    out: Sender,
    username: Option<String>,
}

impl Handler for Session {
    fn on_close(&mut self, _code: ws::CloseCode, _reason: &str) {
        if let Some(ref username) = self.username {
            SOCKETS.write().unwrap().remove(username);
        }
    }

    fn on_message(&mut self, msg: ws::Message) -> ws::Result<()> {
        println!("Server got message '{}'", msg);
        let msg: Inbound = match serde_json::from_str(&msg.into_text()?) {
            Ok(msg) => msg,
            Err(err) => {
                return self.out.send(Outbound::FormatError {
                    error: format!("{:?}", err),
                })
            }
        };

        use Inbound::*;
        match msg {
            Logout => {
                self.username = None;
                self.out.send(Outbound::Success)
            }
            Login { username, password } => {
                // ensure the user is logged out
                self.username = None;

                // only hold the write lock for a short time
                {
                    let mut users = USERS.write().unwrap();
                    if users.contains_key(&username) {
                        if users.get(&username).unwrap() != &password {
                            return self.out.send(Outbound::AuthFail);
                        }
                    } else {
                        users.insert(username.clone(), password);
                    }
                }

                let mut sockets = SOCKETS.write().unwrap();
                self.username = Some(username.clone());
                sockets.insert(username, self.out.clone());
                self.out.send(Outbound::Success)
            }

            Join { channel } => match self.username {
                Some(ref username) => {
                    // ensure the read lock will get dropped before the write lock opens
                    {
                        let mut channels = CHANNELS.read().unwrap();

                        if channels.contains_key(&channel) {
                            let joined = Outbound::Joined {
                                channel: channel.clone(),
                                username: username.clone(),
                            };

                            let mut sockets = SOCKETS.read().unwrap();
                            for user in channels.get(&channel).unwrap() {
                                sockets.get(user).map(|socket| {
                                    let _ = socket.send(joined.clone());
                                });
                            }
                        }
                    }

                    CHANNELS
                        .write()
                        .unwrap()
                        .entry(channel.clone())
                        .or_insert_with(|| HashSet::new())
                        .insert(username.clone());

                    self.out.send(Outbound::Success)
                }

                None => self.out.send(Outbound::NotAuthed),
            },

            Message { channel, text } => match self.username {
                Some(ref username) => {
                    let mut channels = CHANNELS.read().unwrap();

                    if channels.contains_key(&channel) {
                        let message = Outbound::Message {
                            channel: channel.clone(),
                            username: username.clone(),
                            text: text,
                        };

                        let mut sockets = SOCKETS.read().unwrap();
                        for user in channels.get(&channel).unwrap() {
                            sockets.get(user).map(|socket| {
                                let _ = socket.send(message.clone());
                            });
                        }
                    }

                    self.out.send(Outbound::Success)
                }

                None => self.out.send(Outbound::NotAuthed),
            },
        }
    }
}

fn main() {
    env_logger::init();

    while let Err(error) = listen("127.0.0.1:3012", |out| Session {
        out,
        username: None,
    }) {
        error!("Failed to create WebSocket due to {:?}", error);
    }
}
