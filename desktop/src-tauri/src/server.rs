//! Thin HTTP client for the Ludex server (HTTP Basic auth; no CORS since it's native).

use std::time::Duration;

use reqwest::blocking::{Client, Response};
use serde_json::{json, Value};

pub struct Server {
    base: String,
    user: String,
    pass: String,
    http: Client,
}

impl Server {
    pub fn new(base: &str, user: &str, pass: &str) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Server {
            base: base.trim_end_matches('/').to_string(),
            user: user.to_string(),
            pass: pass.to_string(),
            http,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api{}", self.base, path)
    }

    fn get(&self, path: &str) -> Result<Value, String> {
        let resp = self
            .http
            .get(self.url(path))
            .basic_auth(&self.user, Some(&self.pass))
            .send()
            .map_err(net_err)?;
        if !resp.status().is_success() {
            return Err(status_msg(resp.status().as_u16()));
        }
        resp.json().map_err(net_err)
    }

    pub fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        let resp = self
            .http
            .post(self.url(path))
            .basic_auth(&self.user, Some(&self.pass))
            .json(&body)
            .send()
            .map_err(net_err)?;
        if !resp.status().is_success() {
            return Err(status_msg(resp.status().as_u16()));
        }
        resp.json().map_err(net_err)
    }

    pub fn me(&self) -> Result<Value, String> {
        self.get("/auth/me")
    }

    pub fn games(&self) -> Result<Value, String> {
        self.get("/games")
    }

    pub fn game(&self, slug: &str) -> Result<Value, String> {
        self.get(&format!("/games/{slug}"))
    }

    pub fn cover(&self, slug: &str) -> Result<Option<(Vec<u8>, String)>, String> {
        let resp = self
            .http
            .get(self.url(&format!("/games/{slug}/cover")))
            .basic_auth(&self.user, Some(&self.pass))
            .send()
            .map_err(net_err)?;
        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(status_msg(resp.status().as_u16()));
        }
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();
        let bytes = resp.bytes().map_err(net_err)?.to_vec();
        Ok(Some((bytes, ct)))
    }

    /// Streaming download response (caller reads it in chunks).
    pub fn download(&self, slug: &str) -> Result<Response, String> {
        let resp = self
            .http
            .get(self.url(&format!("/download/{slug}")))
            .basic_auth(&self.user, Some(&self.pass))
            .send()
            .map_err(net_err)?;
        if !resp.status().is_success() {
            return Err(status_msg(resp.status().as_u16()));
        }
        Ok(resp)
    }

    pub fn hello(&self, device: &str) {
        let _ = self.post(
            "/agent/hello",
            json!({ "device": device, "platform": "windows", "agent_version": "0.1.0" }),
        );
    }

    pub fn report_installed(&self, device: &str, games: Value) {
        let _ = self.post("/agent/installed", json!({ "device": device, "games": games }));
    }

    pub fn report_session(&self, device: &str, slug: &str, seconds: u64) {
        let _ = self.post(
            "/agent/session",
            json!({ "device": device, "slug": slug, "seconds": seconds }),
        );
    }
}

fn net_err<E: std::fmt::Display>(e: E) -> String {
    format!("Network error: {e}")
}

fn status_msg(code: u16) -> String {
    match code {
        401 => "Invalid username or password".into(),
        403 => "Access denied".into(),
        404 => "Not found on server".into(),
        c => format!("Server error ({c})"),
    }
}
