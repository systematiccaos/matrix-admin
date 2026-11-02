use std::sync::mpsc::{Receiver, Sender};

use eframe::egui;

use crate::config::Profiles;
use crate::matrix::MatrixUser;
use crate::worker::{Command, Event};

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Settings,
    Register,
    Manage,
}

#[derive(Default)]
struct StatusLine {
    text: String,
    is_error: bool,
}

impl StatusLine {
    fn ok(msg: impl Into<String>) -> Self {
        StatusLine { text: msg.into(), is_error: false }
    }
    fn err(msg: impl Into<String>) -> Self {
        StatusLine { text: msg.into(), is_error: true }
    }
    fn show(&self, ui: &mut egui::Ui) {
        if self.text.is_empty() {
            return;
        }
        let color = if self.is_error {
            egui::Color32::from_rgb(220, 80, 80)
        } else {
            egui::Color32::from_rgb(90, 170, 90)
        };
        ui.colored_label(color, &self.text);
    }
}

pub struct MatrixAdminApp {
    profiles: Profiles,
    cmd_tx: Sender<Command>,
    evt_rx: Receiver<Event>,
    tab: Tab,
    busy: u32,

    // Settings tab
    show_secret: bool,
    show_token: bool,
    bootstrap_username: String,
    bootstrap_password: String,
    settings_status: StatusLine,

    // Register tab
    reg_username: String,
    reg_password: String,
    reg_admin: bool,
    register_status: StatusLine,

    // Manage tab
    search: String,
    users: Vec<MatrixUser>,
    total_users: i64,
    selected: Option<String>,
    reset_password_input: String,
    logout_devices: bool,
    deactivate_erase: bool,
    reactivate_password_input: String,
    manage_status: StatusLine,
}

impl MatrixAdminApp {
    pub fn new() -> Self {
        let profiles = Profiles::load();
        let (cmd_tx, evt_rx) = crate::worker::spawn();
        MatrixAdminApp {
            profiles,
            cmd_tx,
            evt_rx,
            tab: Tab::Settings,
            busy: 0,
            show_secret: false,
            show_token: false,
            bootstrap_username: "admin".to_string(),
            bootstrap_password: String::new(),
            settings_status: StatusLine::default(),
            reg_username: String::new(),
            reg_password: String::new(),
            reg_admin: false,
            register_status: StatusLine::default(),
            search: String::new(),
            users: Vec::new(),
            total_users: 0,
            selected: None,
            reset_password_input: String::new(),
            logout_devices: true,
            deactivate_erase: false,
            reactivate_password_input: String::new(),
            manage_status: StatusLine::default(),
        }
    }

    fn send(&mut self, cmd: Command) {
        self.busy += 1;
        let _ = self.cmd_tx.send(cmd);
    }

    /// Clear per-homeserver UI state that must not leak across a profile
    /// switch (a different server's user list, selection, status, etc).
    fn reset_session_state(&mut self) {
        self.users.clear();
        self.total_users = 0;
        self.selected = None;
        self.search.clear();
        self.reset_password_input.clear();
        self.reactivate_password_input.clear();
        self.settings_status = StatusLine::default();
        self.register_status = StatusLine::default();
        self.manage_status = StatusLine::default();
    }

    fn drain_events(&mut self) {
        while let Ok(evt) = self.evt_rx.try_recv() {
            self.busy = self.busy.saturating_sub(1);
            match evt {
                Event::ServerOk => {
                    self.settings_status = StatusLine::ok("Homeserver reachable.");
                }
                Event::ServerError(e) => {
                    self.settings_status = StatusLine::err(format!("Could not reach homeserver: {e}"));
                }
                Event::AdminTokenObtained { user_id, access_token } => {
                    self.profiles.active_mut().admin_user_id = user_id.clone();
                    self.profiles.active_mut().admin_token = access_token;
                    if let Err(e) = self.profiles.save() {
                        self.settings_status = StatusLine::err(format!("Got token but failed to save profiles.json: {e}"));
                    } else {
                        self.settings_status =
                            StatusLine::ok(format!("Admin token obtained for {user_id} and saved"));
                    }
                }
                Event::AdminTokenError(e) => {
                    self.settings_status = StatusLine::err(format!("Could not obtain admin token: {e}"));
                }
                Event::UserRegistered { user_id } => {
                    self.register_status = StatusLine::ok(format!("Registered {user_id}"));
                    self.reg_username.clear();
                    self.reg_password.clear();
                    self.reg_admin = false;
                }
                Event::RegisterError(e) => {
                    self.register_status = StatusLine::err(format!("Registration failed: {e}"));
                }
                Event::UsersListed(list) => {
                    self.total_users = list.total;
                    self.users = list.users;
                    self.manage_status = StatusLine::ok(format!("Loaded {} users", self.users.len()));
                }
                Event::ListUsersError(e) => {
                    self.manage_status = StatusLine::err(format!("Could not list users: {e}"));
                }
                Event::ActionSuccess(msg) => {
                    self.manage_status = StatusLine::ok(msg);
                    self.refresh_users();
                }
                Event::ActionError(e) => {
                    self.manage_status = StatusLine::err(e);
                }
            }
        }
    }

    fn refresh_users(&mut self) {
        let base_url = self.profiles.active().homeserver_url.clone();
        let token = self.profiles.active().admin_token.clone();
        let search = self.search.clone();
        self.send(Command::ListUsers { base_url, token, search });
    }

    fn has_admin_token(&self) -> bool {
        let p = self.profiles.active();
        !p.admin_token.is_empty() && !p.homeserver_url.is_empty()
    }

    fn profile_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Homeserver:");

            let current_name = self.profiles.active().name.clone();
            let mut switch_to: Option<usize> = None;

            egui::ComboBox::new("profile_selector", "")
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    for (i, profile) in self.profiles.items.iter().enumerate() {
                        let label = if profile.name.is_empty() {
                            format!("(unnamed #{})", i + 1)
                        } else {
                            profile.name.clone()
                        };
                        if ui.selectable_label(i == self.profiles.active, label).clicked() {
                            switch_to = Some(i);
                        }
                    }
                });

            if let Some(i) = switch_to {
                if i != self.profiles.active {
                    self.profiles.active = i;
                    let _ = self.profiles.save();
                    self.reset_session_state();
                }
            }

            if ui.button("+ Add homeserver").clicked() {
                self.profiles.add_profile();
                let _ = self.profiles.save();
                self.reset_session_state();
                self.tab = Tab::Settings;
            }

            if self.profiles.items.len() > 1 && ui.button("Delete current").clicked() {
                self.profiles.remove_active();
                let _ = self.profiles.save();
                self.reset_session_state();
            }
        });
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        self.profile_bar(ui);
        ui.separator();

        ui.heading("Homeserver connection");
        ui.add_space(8.0);

        let profile = self.profiles.active_mut();

        egui::Grid::new("settings_grid")
            .num_columns(2)
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                ui.label("Profile name");
                ui.text_edit_singleline(&mut profile.name);
                ui.end_row();

                ui.label("Homeserver URL");
                ui.text_edit_singleline(&mut profile.homeserver_url);
                ui.end_row();

                ui.label("Registration shared secret");
                ui.horizontal(|ui| {
                    let mut edit = egui::TextEdit::singleline(&mut profile.registration_shared_secret);
                    if !self.show_secret {
                        edit = edit.password(true);
                    }
                    ui.add(edit);
                    ui.checkbox(&mut self.show_secret, "show");
                });
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Save settings").clicked() {
                match self.profiles.save() {
                    Ok(()) => self.settings_status = StatusLine::ok("Saved to profiles.json"),
                    Err(e) => self.settings_status = StatusLine::err(format!("Failed to save: {e}")),
                }
            }
            if ui.button("Test connection").clicked() && !self.profiles.active().homeserver_url.is_empty() {
                let base_url = self.profiles.active().homeserver_url.clone();
                self.send(Command::CheckServer { base_url });
            }
        });

        ui.separator();
        ui.heading("Admin access token");
        ui.label(
            "The 'manage users' features need an admin access token. Either bootstrap one using the \
             shared secret above (registers the account if it doesn't exist yet), or log in directly \
             with an existing admin account's username and password.",
        );
        ui.add_space(8.0);

        egui::Grid::new("token_grid")
            .num_columns(2)
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                ui.label("Admin username");
                ui.text_edit_singleline(&mut self.bootstrap_username);
                ui.end_row();

                ui.label("Admin password");
                ui.add(egui::TextEdit::singleline(&mut self.bootstrap_password).password(true));
                ui.end_row();

                ui.label("Current admin token");
                ui.horizontal(|ui| {
                    let mut edit =
                        egui::TextEdit::singleline(&mut self.profiles.active_mut().admin_token).desired_width(300.0);
                    if !self.show_token {
                        edit = edit.password(true);
                    }
                    ui.add(edit);
                    ui.checkbox(&mut self.show_token, "show");
                });
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let can_bootstrap = !self.profiles.active().homeserver_url.is_empty()
                && !self.profiles.active().registration_shared_secret.is_empty()
                && !self.bootstrap_username.is_empty()
                && !self.bootstrap_password.is_empty();
            if ui
                .add_enabled(can_bootstrap, egui::Button::new("Bootstrap / fetch admin token"))
                .clicked()
            {
                self.send(Command::BootstrapAdmin {
                    base_url: self.profiles.active().homeserver_url.clone(),
                    shared_secret: self.profiles.active().registration_shared_secret.clone(),
                    username: self.bootstrap_username.clone(),
                    password: self.bootstrap_password.clone(),
                });
            }

            let can_login = !self.profiles.active().homeserver_url.is_empty()
                && !self.bootstrap_username.is_empty()
                && !self.bootstrap_password.is_empty();
            if ui
                .add_enabled(can_login, egui::Button::new("Log in (existing account)"))
                .clicked()
            {
                self.send(Command::LoginAdmin {
                    base_url: self.profiles.active().homeserver_url.clone(),
                    username: self.bootstrap_username.clone(),
                    password: self.bootstrap_password.clone(),
                });
            }

            if ui.button("Clear token").clicked() {
                let profile = self.profiles.active_mut();
                profile.admin_token.clear();
                profile.admin_user_id.clear();
                let _ = self.profiles.save();
            }
        });

        if !self.profiles.active().admin_user_id.is_empty() {
            ui.label(format!("Current admin identity: {}", self.profiles.active().admin_user_id));
        }

        ui.add_space(8.0);
        self.settings_status.show(ui);
    }

    fn register_tab(&mut self, ui: &mut egui::Ui) {
        self.profile_bar(ui);
        ui.separator();
        ui.heading("Register a new user");

        if self.profiles.active().homeserver_url.is_empty()
            || self.profiles.active().registration_shared_secret.is_empty()
        {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 60),
                "Set the homeserver URL and registration shared secret in Settings first.",
            );
            ui.add_space(8.0);
        }

        egui::Grid::new("register_grid")
            .num_columns(2)
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                ui.label("Username (localpart)");
                ui.text_edit_singleline(&mut self.reg_username);
                ui.end_row();

                ui.label("Password");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.reg_password).password(true));
                    if ui.button("Generate").clicked() {
                        self.reg_password = generate_password();
                    }
                });
                ui.end_row();

                ui.label("Grant server admin");
                ui.checkbox(&mut self.reg_admin, "");
                ui.end_row();
            });

        ui.add_space(8.0);
        let can_register = !self.profiles.active().homeserver_url.is_empty()
            && !self.profiles.active().registration_shared_secret.is_empty()
            && !self.reg_username.is_empty()
            && !self.reg_password.is_empty();

        if ui
            .add_enabled(can_register, egui::Button::new("Register user"))
            .clicked()
        {
            self.send(Command::RegisterUser {
                base_url: self.profiles.active().homeserver_url.clone(),
                shared_secret: self.profiles.active().registration_shared_secret.clone(),
                username: self.reg_username.clone(),
                password: self.reg_password.clone(),
                admin: self.reg_admin,
            });
        }

        ui.add_space(8.0);
        self.register_status.show(ui);
    }

    fn manage_tab(&mut self, ui: &mut egui::Ui) {
        self.profile_bar(ui);
        ui.separator();
        ui.heading("Manage users");

        if !self.has_admin_token() {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 60),
                "Obtain an admin token in Settings first.",
            );
            return;
        }

        ui.horizontal(|ui| {
            ui.label("Search:");
            let response = ui.text_edit_singleline(&mut self.search);
            let search_now = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Refresh").clicked() || search_now {
                self.refresh_users();
            }
        });
        ui.label(format!("{} users loaded (server reports {} total)", self.users.len(), self.total_users));
        ui.separator();

        let mut clicked_user: Option<String> = None;

        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            egui::Grid::new("users_table").striped(true).num_columns(5).show(ui, |ui| {
                ui.strong("User ID");
                ui.strong("Display name");
                ui.strong("Admin");
                ui.strong("Deactivated");
                ui.strong("Locked");
                ui.end_row();

                for user in &self.users {
                    let is_selected = self.selected.as_deref() == Some(user.name.as_str());
                    if ui.selectable_label(is_selected, &user.name).clicked() {
                        clicked_user = Some(user.name.clone());
                    }
                    ui.label(user.displayname.clone().unwrap_or_default());
                    ui.label(if user.admin { "yes" } else { "" });
                    ui.label(if user.deactivated { "yes" } else { "" });
                    ui.label(if user.locked { "yes" } else { "" });
                    ui.end_row();
                }
            });
        });

        if let Some(user_id) = clicked_user {
            self.selected = Some(user_id);
            self.reset_password_input.clear();
            self.reactivate_password_input.clear();
        }

        ui.separator();
        self.manage_status.show(ui);
        ui.separator();

        let Some(selected_id) = self.selected.clone() else {
            ui.label("Select a user above to manage them.");
            return;
        };

        let Some(user) = self.users.iter().find(|u| u.name == selected_id).cloned() else {
            self.selected = None;
            return;
        };

        ui.heading(&user.name);

        let base_url = self.profiles.active().homeserver_url.clone();
        let token = self.profiles.active().admin_token.clone();

        ui.horizontal(|ui| {
            ui.label("Reset password:");
            ui.add(egui::TextEdit::singleline(&mut self.reset_password_input).password(true));
            ui.checkbox(&mut self.logout_devices, "log out other devices");
            if ui
                .add_enabled(!self.reset_password_input.is_empty(), egui::Button::new("Apply"))
                .clicked()
            {
                self.send(Command::ResetPassword {
                    base_url: base_url.clone(),
                    token: token.clone(),
                    user_id: user.name.clone(),
                    new_password: self.reset_password_input.clone(),
                    logout_devices: self.logout_devices,
                });
            }
        });

        ui.horizontal(|ui| {
            if user.deactivated {
                ui.label("Reactivate with new password:");
                ui.add(egui::TextEdit::singleline(&mut self.reactivate_password_input).password(true));
                if ui
                    .add_enabled(!self.reactivate_password_input.is_empty(), egui::Button::new("Reactivate"))
                    .clicked()
                {
                    self.send(Command::Reactivate {
                        base_url: base_url.clone(),
                        token: token.clone(),
                        user_id: user.name.clone(),
                        new_password: self.reactivate_password_input.clone(),
                    });
                }
            } else {
                ui.checkbox(&mut self.deactivate_erase, "erase personal data");
                if ui.button("Deactivate").clicked() {
                    self.send(Command::Deactivate {
                        base_url: base_url.clone(),
                        token: token.clone(),
                        user_id: user.name.clone(),
                        erase: self.deactivate_erase,
                    });
                }
            }
        });

        ui.horizontal(|ui| {
            if user.admin {
                if ui.button("Revoke admin").clicked() {
                    self.send(Command::SetAdmin {
                        base_url: base_url.clone(),
                        token: token.clone(),
                        user_id: user.name.clone(),
                        admin: false,
                    });
                }
            } else if ui.button("Grant admin").clicked() {
                self.send(Command::SetAdmin {
                    base_url: base_url.clone(),
                    token: token.clone(),
                    user_id: user.name.clone(),
                    admin: true,
                });
            }

            if user.locked {
                if ui.button("Unlock").clicked() {
                    self.send(Command::SetLocked {
                        base_url: base_url.clone(),
                        token: token.clone(),
                        user_id: user.name.clone(),
                        locked: false,
                    });
                }
            } else if ui.button("Lock").clicked() {
                self.send(Command::SetLocked {
                    base_url: base_url.clone(),
                    token: token.clone(),
                    user_id: user.name.clone(),
                    locked: true,
                });
            }
        });
    }
}

impl eframe::App for MatrixAdminApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_events();
        if self.busy > 0 {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(80));
        }

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.selectable_value(&mut self.tab, Tab::Settings, "Settings");
                ui.selectable_value(&mut self.tab, Tab::Register, "Register user");
                ui.selectable_value(&mut self.tab, Tab::Manage, "Manage users");
                if self.busy > 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spinner();
                    });
                }
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Settings => self.settings_tab(ui),
            Tab::Register => self.register_tab(ui),
            Tab::Manage => self.manage_tab(ui),
        });
    }
}

fn generate_password() -> String {
    use rand::RngExt;
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*";
    let mut rng = rand::rng();
    (0..20).map(|_| CHARS[rng.random_range(0..CHARS.len())] as char).collect()
}
