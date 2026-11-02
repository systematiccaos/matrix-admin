use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const PROFILES_FILE: &str = "profiles.json";
const LEGACY_ENV_FILE: &str = ".env";

const LEGACY_KEY_HOMESERVER_URL: &str = "MATRIX_HOMESERVER_URL";
const LEGACY_KEY_SHARED_SECRET: &str = "MATRIX_REGISTRATION_SHARED_SECRET";
const LEGACY_KEY_ADMIN_TOKEN: &str = "MATRIX_ADMIN_TOKEN";
const LEGACY_KEY_ADMIN_USER_ID: &str = "MATRIX_ADMIN_USER_ID";

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub homeserver_url: String,
    pub registration_shared_secret: String,
    pub admin_token: String,
    pub admin_user_id: String,
}

#[derive(Serialize, Deserialize)]
struct ProfilesFile {
    active: usize,
    profiles: Vec<Profile>,
}

pub struct Profiles {
    pub items: Vec<Profile>,
    pub active: usize,
}

impl Profiles {
    pub fn load() -> Self {
        let path = PathBuf::from(PROFILES_FILE);
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(parsed) = serde_json::from_str::<ProfilesFile>(&text) {
                if !parsed.profiles.is_empty() {
                    let active = parsed.active.min(parsed.profiles.len() - 1);
                    return Profiles { items: parsed.profiles, active };
                }
            }
        }

        if let Some(profile) = Self::migrate_legacy_env() {
            let profiles = Profiles { items: vec![profile], active: 0 };
            let _ = profiles.save();
            return profiles;
        }

        Profiles {
            items: vec![Profile { name: "Default".to_string(), ..Default::default() }],
            active: 0,
        }
    }

    /// One-time migration from the old single-homeserver `.env` file used
    /// before multi-homeserver support was added.
    fn migrate_legacy_env() -> Option<Profile> {
        let path = PathBuf::from(LEGACY_ENV_FILE);
        if !path.exists() {
            return None;
        }
        let _ = dotenvy::from_path(&path);

        let homeserver_url = std::env::var(LEGACY_KEY_HOMESERVER_URL).unwrap_or_default();
        let registration_shared_secret = std::env::var(LEGACY_KEY_SHARED_SECRET).unwrap_or_default();
        let admin_token = std::env::var(LEGACY_KEY_ADMIN_TOKEN).unwrap_or_default();
        let admin_user_id = std::env::var(LEGACY_KEY_ADMIN_USER_ID).unwrap_or_default();

        if homeserver_url.is_empty() && registration_shared_secret.is_empty() && admin_token.is_empty() {
            return None;
        }

        Some(Profile {
            name: "Default".to_string(),
            homeserver_url,
            registration_shared_secret,
            admin_token,
            admin_user_id,
        })
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let file = ProfilesFile { active: self.active, profiles: self.items.clone() };
        let text = serde_json::to_string_pretty(&file)?;
        fs::write(PROFILES_FILE, text)?;
        Ok(())
    }

    pub fn active(&self) -> &Profile {
        &self.items[self.active]
    }

    pub fn active_mut(&mut self) -> &mut Profile {
        &mut self.items[self.active]
    }

    pub fn add_profile(&mut self) {
        self.items.push(Profile {
            name: format!("Homeserver {}", self.items.len() + 1),
            ..Default::default()
        });
        self.active = self.items.len() - 1;
    }

    /// Remove the active profile. If it's the only one left, it is reset to
    /// a blank profile instead of leaving the app with no profiles at all.
    pub fn remove_active(&mut self) {
        if self.items.len() <= 1 {
            self.items[0] = Profile { name: "Default".to_string(), ..Default::default() };
            self.active = 0;
            return;
        }
        self.items.remove(self.active);
        if self.active >= self.items.len() {
            self.active = self.items.len() - 1;
        }
    }
}
