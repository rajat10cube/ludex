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
        // No *total* timeout: downloads run for many minutes. Only bound how long
        // we wait to establish the connection; per-call timeouts are added to the
        // small API requests below (but never to the download stream).
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(30))
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
        self.get_query(path, &[])
    }

    fn get_query(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, String> {
        let resp = self
            .http
            .get(self.url(path))
            .query(query)
            .basic_auth(&self.user, Some(&self.pass))
            .timeout(Duration::from_secs(30))
            .send()
            .map_err(net_err)?;
        read_json(resp)
    }

    pub fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        let resp = self
            .http
            .post(self.url(path))
            .basic_auth(&self.user, Some(&self.pass))
            .json(&body)
            .timeout(Duration::from_secs(30))
            .send()
            .map_err(net_err)?;
        read_json(resp)
    }

    fn delete(&self, path: &str) -> Result<(), String> {
        let resp = self
            .http
            .delete(self.url(path))
            .basic_auth(&self.user, Some(&self.pass))
            .timeout(Duration::from_secs(30))
            .send()
            .map_err(net_err)?;
        if !resp.status().is_success() {
            return Err(api_err(resp));
        }
        Ok(())
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
            .timeout(Duration::from_secs(30))
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

    /// Streaming download response (caller reads it in chunks). `offset` resumes:
    /// folder games use `?skip=`, loose files use an HTTP `Range` header.
    pub fn download(&self, slug: &str, offset: u64, is_tar: bool) -> Result<Response, String> {
        let mut rb = self
            .http
            .get(self.url(&format!("/download/{slug}")))
            .basic_auth(&self.user, Some(&self.pass));
        if offset > 0 {
            rb = if is_tar {
                rb.query(&[("skip", offset.to_string())])
            } else {
                rb.header("Range", format!("bytes={offset}-"))
            };
        }
        let resp = rb.send().map_err(net_err)?;
        if !resp.status().is_success() {
            return Err(status_msg(resp.status().as_u16()));
        }
        Ok(resp)
    }

    // --- library management (admin) ---
    pub fn libraries(&self) -> Result<Value, String> {
        self.get("/libraries")
    }

    pub fn add_library(&self, path: &str, name: Option<String>) -> Result<Value, String> {
        self.post("/libraries", json!({ "path": path, "name": name }))
    }

    pub fn delete_library(&self, id: i64) -> Result<(), String> {
        self.delete(&format!("/libraries/{id}"))
    }

    pub fn scan(&self) -> Result<Value, String> {
        self.post("/libraries/scan", json!({}))
    }

    pub fn scan_status(&self) -> Result<Value, String> {
        self.get("/libraries/scan/status")
    }

    /// Browse folders *on the server* — an empty path lists the roots.
    pub fn browse(&self, path: &str) -> Result<Value, String> {
        self.get_query("/libraries/browse", &[("path", path)])
    }

    pub fn artwork_settings(&self) -> Result<Value, String> {
        self.get("/settings/artwork")
    }

    pub fn save_artwork_settings(&self, body: Value) -> Result<Value, String> {
        self.post("/settings/artwork", body)
    }

    pub fn refresh_artwork(&self) -> Result<Value, String> {
        self.post("/libraries/artwork/refresh", json!({}))
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

fn read_json(resp: Response) -> Result<Value, String> {
    if !resp.status().is_success() {
        return Err(api_err(resp));
    }
    // 202/204 bodies may be empty — treat that as null rather than an error.
    let text = resp.text().map_err(net_err)?;
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| format!("Bad response from server: {e}"))
}

/// Prefer FastAPI's `detail` message ("Not a directory…") over a bare status code.
fn api_err(resp: Response) -> String {
    let code = resp.status().as_u16();
    if let Ok(v) = resp.json::<Value>() {
        if let Some(detail) = v.get("detail").and_then(|d| d.as_str()) {
            return detail.to_string();
        }
    }
    status_msg(code)
}

fn net_err<E: std::fmt::Display>(e: E) -> String {
    format!("Network error: {e}")
}

fn status_msg(code: u16) -> String {
    match code {
        401 => "Invalid username or password".into(),
        403 => "Access denied — this needs an admin account".into(),
        404 => "Not found on server".into(),
        409 => "Already exists".into(),
        c => format!("Server error ({c})"),
    }
}
