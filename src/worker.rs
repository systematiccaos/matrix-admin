use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use crate::matrix::{MatrixClient, UserList};

pub enum Command {
    CheckServer {
        base_url: String,
    },
    BootstrapAdmin {
        base_url: String,
        shared_secret: String,
        username: String,
        password: String,
    },
    LoginAdmin {
        base_url: String,
        username: String,
        password: String,
    },
    RegisterUser {
        base_url: String,
        shared_secret: String,
        username: String,
        password: String,
        admin: bool,
    },
    ListUsers {
        base_url: String,
        token: String,
        search: String,
    },
    ResetPassword {
        base_url: String,
        token: String,
        user_id: String,
        new_password: String,
        logout_devices: bool,
    },
    Deactivate {
        base_url: String,
        token: String,
        user_id: String,
        erase: bool,
    },
    Reactivate {
        base_url: String,
        token: String,
        user_id: String,
        new_password: String,
    },
    SetAdmin {
        base_url: String,
        token: String,
        user_id: String,
        admin: bool,
    },
    SetLocked {
        base_url: String,
        token: String,
        user_id: String,
        locked: bool,
    },
}

pub enum Event {
    ServerOk,
    ServerError(String),
    AdminTokenObtained { user_id: String, access_token: String },
    AdminTokenError(String),
    UserRegistered { user_id: String },
    RegisterError(String),
    UsersListed(UserList),
    ListUsersError(String),
    ActionSuccess(String),
    ActionError(String),
}

pub fn spawn() -> (Sender<Command>, Receiver<Event>) {
    let (cmd_tx, cmd_rx) = channel::<Command>();
    let (evt_tx, evt_rx) = channel::<Event>();

    thread::spawn(move || {
        for cmd in cmd_rx {
            handle(cmd, &evt_tx);
        }
    });

    (cmd_tx, evt_rx)
}

fn handle(cmd: Command, evt_tx: &Sender<Event>) {
    match cmd {
        Command::CheckServer { base_url } => {
            let client = MatrixClient::new(&base_url);
            let event = match client.check_server() {
                Ok(()) => Event::ServerOk,
                Err(e) => Event::ServerError(e.to_string()),
            };
            let _ = evt_tx.send(event);
        }

        Command::BootstrapAdmin {
            base_url,
            shared_secret,
            username,
            password,
        } => {
            let client = MatrixClient::new(&base_url);
            let event = match client.register_with_shared_secret(&shared_secret, &username, &password, true) {
                Ok(user) => Event::AdminTokenObtained {
                    user_id: user.user_id,
                    access_token: user.access_token,
                },
                Err(register_err) => {
                    // Most likely cause: the admin account already exists.
                    // Fall back to a normal password login with the same credentials.
                    match client.login(&username, &password) {
                        Ok(user) => Event::AdminTokenObtained {
                            user_id: user.user_id,
                            access_token: user.access_token,
                        },
                        Err(login_err) => Event::AdminTokenError(format!(
                            "registration failed ({register_err}); login fallback also failed ({login_err})"
                        )),
                    }
                }
            };
            let _ = evt_tx.send(event);
        }

        Command::LoginAdmin {
            base_url,
            username,
            password,
        } => {
            let client = MatrixClient::new(&base_url);
            let event = match client.login(&username, &password) {
                Ok(user) => Event::AdminTokenObtained {
                    user_id: user.user_id,
                    access_token: user.access_token,
                },
                Err(e) => Event::AdminTokenError(e.to_string()),
            };
            let _ = evt_tx.send(event);
        }

        Command::RegisterUser {
            base_url,
            shared_secret,
            username,
            password,
            admin,
        } => {
            let client = MatrixClient::new(&base_url);
            let event = match client.register_with_shared_secret(&shared_secret, &username, &password, admin) {
                Ok(user) => Event::UserRegistered { user_id: user.user_id },
                Err(e) => Event::RegisterError(e.to_string()),
            };
            let _ = evt_tx.send(event);
        }

        Command::ListUsers { base_url, token, search } => {
            let client = MatrixClient::new(&base_url);
            let event = match client.list_users(&token, &search) {
                Ok(list) => Event::UsersListed(list),
                Err(e) => Event::ListUsersError(e.to_string()),
            };
            let _ = evt_tx.send(event);
        }

        Command::ResetPassword {
            base_url,
            token,
            user_id,
            new_password,
            logout_devices,
        } => {
            let client = MatrixClient::new(&base_url);
            let event = match client.reset_password(&token, &user_id, &new_password, logout_devices) {
                Ok(()) => Event::ActionSuccess(format!("Password reset for {user_id}")),
                Err(e) => Event::ActionError(e.to_string()),
            };
            let _ = evt_tx.send(event);
        }

        Command::Deactivate {
            base_url,
            token,
            user_id,
            erase,
        } => {
            let client = MatrixClient::new(&base_url);
            let event = match client.deactivate(&token, &user_id, erase) {
                Ok(()) => Event::ActionSuccess(format!("Deactivated {user_id}")),
                Err(e) => Event::ActionError(e.to_string()),
            };
            let _ = evt_tx.send(event);
        }

        Command::Reactivate {
            base_url,
            token,
            user_id,
            new_password,
        } => {
            let client = MatrixClient::new(&base_url);
            let event = match client.reactivate(&token, &user_id, &new_password) {
                Ok(()) => Event::ActionSuccess(format!("Reactivated {user_id}")),
                Err(e) => Event::ActionError(e.to_string()),
            };
            let _ = evt_tx.send(event);
        }

        Command::SetAdmin {
            base_url,
            token,
            user_id,
            admin,
        } => {
            let client = MatrixClient::new(&base_url);
            let event = match client.set_admin(&token, &user_id, admin) {
                Ok(()) => Event::ActionSuccess(format!(
                    "{user_id} is now {}",
                    if admin { "an admin" } else { "not an admin" }
                )),
                Err(e) => Event::ActionError(e.to_string()),
            };
            let _ = evt_tx.send(event);
        }

        Command::SetLocked {
            base_url,
            token,
            user_id,
            locked,
        } => {
            let client = MatrixClient::new(&base_url);
            let event = match client.set_locked(&token, &user_id, locked) {
                Ok(()) => Event::ActionSuccess(format!(
                    "{user_id} is now {}",
                    if locked { "locked" } else { "unlocked" }
                )),
                Err(e) => Event::ActionError(e.to_string()),
            };
            let _ = evt_tx.send(event);
        }
    }
}
