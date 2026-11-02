use anyhow::{anyhow, bail, Context, Result};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone, Deserialize)]
pub struct MatrixUser {
    pub name: String,
    #[serde(default)]
    pub displayname: Option<String>,
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub deactivated: bool,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Clone)]
pub struct UserList {
    pub users: Vec<MatrixUser>,
    pub total: i64,
}

#[derive(Debug, Clone)]
pub struct RegisteredUser {
    pub user_id: String,
    pub access_token: String,
}

#[derive(Debug, Deserialize)]
struct NonceResponse {
    nonce: String,
}

#[derive(Debug, Serialize)]
struct RegisterRequest {
    nonce: String,
    username: String,
    password: String,
    admin: bool,
    mac: String,
}

#[derive(Debug, Deserialize)]
struct RegisterResponse {
    user_id: String,
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    user_id: String,
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct UsersResponse {
    users: Vec<MatrixUser>,
    #[serde(default)]
    total: i64,
}

#[derive(Debug, Deserialize)]
struct MatrixErrorBody {
    #[serde(default)]
    error: String,
    #[serde(default)]
    errcode: String,
}

pub struct MatrixClient {
    http: reqwest::blocking::Client,
    base_url: String,
}

impl MatrixClient {
    pub fn new(base_url: &str) -> Self {
        let http = reqwest::blocking::Client::builder()
            .build()
            .expect("failed to build HTTP client");
        MatrixClient {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Extract a readable error message from a non-2xx Matrix API response.
    fn api_error(resp: reqwest::blocking::Response) -> anyhow::Error {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        if let Ok(parsed) = serde_json::from_str::<MatrixErrorBody>(&body) {
            if !parsed.error.is_empty() {
                return anyhow!("{} ({}): {}", status, parsed.errcode, parsed.error);
            }
        }
        anyhow!("{}: {}", status, body)
    }

    /// Register a new user via the Synapse admin shared-secret registration flow.
    /// Does not require an existing admin access token.
    pub fn register_with_shared_secret(
        &self,
        shared_secret: &str,
        username: &str,
        password: &str,
        admin: bool,
    ) -> Result<RegisteredUser> {
        let nonce_resp: NonceResponse = self
            .http
            .get(self.url("/_synapse/admin/v1/register"))
            .send()
            .context("requesting registration nonce")?
            .error_for_status()
            .context("server rejected nonce request")?
            .json()
            .context("parsing nonce response")?;

        let nonce = nonce_resp.nonce;

        let mut mac = HmacSha1::new_from_slice(shared_secret.as_bytes())
            .map_err(|e| anyhow!("invalid shared secret: {e}"))?;
        mac.update(nonce.as_bytes());
        mac.update(b"\x00");
        mac.update(username.as_bytes());
        mac.update(b"\x00");
        mac.update(password.as_bytes());
        mac.update(b"\x00");
        mac.update(if admin { b"admin" } else { b"notadmin" });
        let mac_hex = hex::encode(mac.finalize().into_bytes());

        let req = RegisterRequest {
            nonce,
            username: username.to_string(),
            password: password.to_string(),
            admin,
            mac: mac_hex,
        };

        let resp = self
            .http
            .post(self.url("/_synapse/admin/v1/register"))
            .json(&req)
            .send()
            .context("sending registration request")?;

        if !resp.status().is_success() {
            return Err(Self::api_error(resp));
        }

        let body: RegisterResponse = resp.json().context("parsing registration response")?;
        Ok(RegisteredUser {
            user_id: body.user_id,
            access_token: body.access_token,
        })
    }

    /// Log in as an existing user/password to obtain an access token.
    pub fn login(&self, username: &str, password: &str) -> Result<RegisteredUser> {
        let body = serde_json::json!({
            "type": "m.login.password",
            "identifier": { "type": "m.id.user", "user": username },
            "password": password,
        });

        let resp = self
            .http
            .post(self.url("/_matrix/client/v3/login"))
            .json(&body)
            .send()
            .context("sending login request")?;

        if !resp.status().is_success() {
            return Err(Self::api_error(resp));
        }

        let parsed: LoginResponse = resp.json().context("parsing login response")?;
        Ok(RegisteredUser {
            user_id: parsed.user_id,
            access_token: parsed.access_token,
        })
    }

    fn authed(&self, token: &str, builder: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
        builder.bearer_auth(token)
    }

    pub fn list_users(&self, token: &str, search: &str) -> Result<UserList> {
        let mut req = self
            .http
            .get(self.url("/_synapse/admin/v2/users"))
            .query(&[("from", "0"), ("limit", "200"), ("deactivated", "true")]);
        if !search.trim().is_empty() {
            req = req.query(&[("name", search.trim())]);
        }
        let resp = self.authed(token, req).send().context("listing users")?;

        if !resp.status().is_success() {
            return Err(Self::api_error(resp));
        }

        let parsed: UsersResponse = resp.json().context("parsing users list")?;
        Ok(UserList {
            users: parsed.users,
            total: parsed.total,
        })
    }

    pub fn reset_password(&self, token: &str, user_id: &str, new_password: &str, logout_devices: bool) -> Result<()> {
        let body = serde_json::json!({
            "new_password": new_password,
            "logout_devices": logout_devices,
        });
        let resp = self
            .authed(
                token,
                self.http
                    .post(self.url(&format!("/_synapse/admin/v1/reset_password/{user_id}"))),
            )
            .json(&body)
            .send()
            .context("resetting password")?;

        if !resp.status().is_success() {
            return Err(Self::api_error(resp));
        }
        Ok(())
    }

    pub fn deactivate(&self, token: &str, user_id: &str, erase: bool) -> Result<()> {
        let body = serde_json::json!({ "erase": erase });
        let resp = self
            .authed(
                token,
                self.http
                    .post(self.url(&format!("/_synapse/admin/v1/deactivate/{user_id}"))),
            )
            .json(&body)
            .send()
            .context("deactivating user")?;

        if !resp.status().is_success() {
            return Err(Self::api_error(resp));
        }
        Ok(())
    }

    /// Reactivate a deactivated user. Synapse requires a fresh password to be
    /// set at the same time, since deactivation erases the old one.
    pub fn reactivate(&self, token: &str, user_id: &str, new_password: &str) -> Result<()> {
        let body = serde_json::json!({
            "deactivated": false,
            "password": new_password,
        });
        let resp = self
            .authed(
                token,
                self.http.put(self.url(&format!("/_synapse/admin/v2/users/{user_id}"))),
            )
            .json(&body)
            .send()
            .context("reactivating user")?;

        if !resp.status().is_success() {
            return Err(Self::api_error(resp));
        }
        Ok(())
    }

    pub fn set_admin(&self, token: &str, user_id: &str, admin: bool) -> Result<()> {
        self.patch_user(token, user_id, serde_json::json!({ "admin": admin }))
    }

    pub fn set_locked(&self, token: &str, user_id: &str, locked: bool) -> Result<()> {
        self.patch_user(token, user_id, serde_json::json!({ "locked": locked }))
    }

    fn patch_user(&self, token: &str, user_id: &str, body: serde_json::Value) -> Result<()> {
        let resp = self
            .authed(
                token,
                self.http.put(self.url(&format!("/_synapse/admin/v2/users/{user_id}"))),
            )
            .json(&body)
            .send()
            .context("updating user")?;

        if !resp.status().is_success() {
            return Err(Self::api_error(resp));
        }
        Ok(())
    }

    /// Sanity check that the homeserver URL is reachable and looks like a
    /// Matrix server.
    pub fn check_server(&self) -> Result<()> {
        let resp = self
            .http
            .get(self.url("/_matrix/client/versions"))
            .send()
            .context("reaching homeserver")?;
        if !resp.status().is_success() {
            bail!("homeserver returned {}", resp.status());
        }
        Ok(())
    }
}
