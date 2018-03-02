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

#[derive(Clone, PartialEq)]
struct User {
    password: String,
    channels: HashSet<String>,
}

lazy_static! {
    static ref SOCKETS: RwLock<HashMap<String, Sender>> =  RwLock::new(HashMap::new());
    static ref USERS: RwLock<HashMap<String, User>> = RwLock::new(HashMap::new());
    static ref CHANNELS: RwLock<HashMap<String, HashSet<String>>> = RwLock::new(HashMap::new());
}

mod messages;
use messages::*;

fn send_to_channel(channel: String, message: Outbound) {
    let channels = CHANNELS.read().unwrap();

    if channels.contains_key(&channel) {
        let sockets = SOCKETS.read().unwrap();
        for user in channels.get(&channel).unwrap() {
            sockets.get(user).map(|socket| {
                let _ = socket.send(message.clone());
            });
        }
    }
}

struct Session {
    out: Sender,
    username: Option<String>,
}

impl Session {
    fn authed(&self) -> Result<&str, ws::Error> {
        match self.username {
            Some(ref username) => Ok(username),
            None => {
                let _ = self.out.send(Outbound::NotAuthed);
                Err(ws::Error::new(ws::ErrorKind::Internal, "NotAuthed"))
            }
        }
    }
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
                    error: &format!("{:?}", err),
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

                let mut users = USERS.write().unwrap();
                if users.contains_key(&username) {
                    if users.get(&username).unwrap().password != password {
                        return self.out.send(Outbound::AuthFail);
                    }
                } else {
                    users.insert(
                        username.clone(),
                        User {
                            password,
                            channels: HashSet::new(),
                        },
                    );
                }

                let mut sockets = SOCKETS.write().unwrap();
                self.username = Some(username.clone());
                sockets.insert(username, self.out.clone());

                self.out.send(Outbound::Success)
            }

            Join { channel } => {
                let username = self.authed()?;
                send_to_channel(
                    channel.clone(),
                    Outbound::Joined {
                        username,
                        channel: &channel,
                    },
                );

                USERS
                    .write()
                    .unwrap()
                    .get_mut(username)
                    .unwrap()
                    .channels
                    .insert(channel.clone());

                CHANNELS
                    .write()
                    .unwrap()
                    .entry(channel)
                    .or_insert_with(|| HashSet::new())
                    .insert(username.to_string());

                self.out.send(Outbound::Success)
            }

            Leave { channel } => {
                let username = self.authed()?;
                send_to_channel(
                    channel.clone(),
                    Outbound::Left {
                        username,
                        channel: &channel,
                    },
                );

                USERS
                    .write()
                    .unwrap()
                    .get_mut(username)
                    .unwrap()
                    .channels
                    .remove(&channel);

                CHANNELS
                    .write()
                    .unwrap()
                    .entry(channel)
                    .or_insert_with(|| HashSet::new())
                    .remove(username);

                self.out.send(Outbound::Success)
            }

            ChannelInfo { channel } => {
                let _ = self.authed()?;
                if let Some(members) = CHANNELS.read().unwrap().get(&channel) {
                    self.out.send(Outbound::ChannelInfo {
                        members: members.iter().map(|x| x.as_str()).collect(),
                    })
                } else {
                    self.out.send(Outbound::NoSuchChannel)
                }
            }

            JoinedChannels => {
                let username = self.authed()?;
                let users = USERS.read().unwrap();
                let user = users.get(username).unwrap();
                self.out.send(Outbound::Channels {
                    channels: user.channels
                        .iter()
                        .map(|channel| channel.as_str())
                        .collect(),
                })
            }

            AllChannels => {
                let _ = self.authed()?;
                let channels = CHANNELS.read().unwrap();

                self.out.send(Outbound::Channels {
                    channels: channels
                        .iter()
                        .map(|(channel, _)| channel.as_str())
                        .collect(),
                })
            }

            Message { channel, text } => {
                let username = self.authed()?;
                match CHANNELS.read().unwrap().get(&channel) {
                    Some(members) if members.contains(username) => {
                        send_to_channel(
                            channel.clone(),
                            Outbound::Message {
                                username,
                                channel: &channel,
                                text: &text,
                            },
                        );

                        self.out.send(Outbound::Success)
                    }

                    _ => self.out.send(Outbound::NotInChannel),
                }
            }
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
