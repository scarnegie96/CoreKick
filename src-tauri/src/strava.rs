//! Strava: OAuth (desktop loopback flow) and activity upload.
//!
//! Strava only issues API credentials per *application*, so you register your
//! own (free) app at https://www.strava.com/settings/api and paste its Client
//! ID and Secret into CoreKick. A new app is limited to one athlete – you –
//! which is all a personal app needs. Set "Authorization Callback Domain" to
//! `localhost` (Strava also white-lists 127.0.0.1).
//!
//! Flow: we open the browser at Strava's authorize page with a redirect to a
//! one-shot HTTP listener on 127.0.0.1, receive `?code=…`, exchange it for
//! tokens and keep them in the local database. Tokens are refreshed
//! automatically. Uploads go as FIT files (Virtual Ride) and are polled until
//! Strava has processed them.

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::AppState;

const OAUTH_BASE: &str = "https://www.strava.com/oauth";
/// Current API host. Strava's new host (`https://api-v3.strava.com`) is
/// available from 4 Jan 2027 and mandatory from 1 Jun 2027 – override with
/// the `strava_api_base` setting when switching.
const DEFAULT_API_BASE: &str = "https://www.strava.com/api/v3";

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub athlete_id: u64,
    pub athlete_name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct StravaStatus {
    pub configured: bool,
    pub connected: bool,
    pub client_id: Option<String>,
    pub athlete_name: Option<String>,
    pub auto_upload: bool,
    pub api_base: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct UploadEvent {
    pub ride_id: String,
    pub status: String,
    pub activity_id: Option<u64>,
    pub error: Option<String>,
}

fn meta(app: &AppHandle, key: &str) -> Option<String> {
    app.state::<AppState>().store.lock().unwrap().meta(key).ok().flatten().filter(|s| !s.is_empty())
}

fn set_meta(app: &AppHandle, key: &str, v: &str) -> Result<()> {
    app.state::<AppState>().store.lock().unwrap().set_meta(key, v)
}

fn api_base(app: &AppHandle) -> String {
    meta(app, "strava_api_base").unwrap_or_else(|| DEFAULT_API_BASE.to_string()).trim_end_matches('/').to_string()
}

fn tokens(app: &AppHandle) -> Option<Tokens> {
    meta(app, "strava_tokens").and_then(|j| serde_json::from_str(&j).ok())
}

fn client(app: &AppHandle) -> Option<(String, String)> {
    Some((meta(app, "strava_client_id")?, meta(app, "strava_client_secret")?))
}

pub fn status(app: &AppHandle) -> StravaStatus {
    let t = tokens(app);
    StravaStatus {
        configured: client(app).is_some(),
        connected: t.is_some(),
        client_id: meta(app, "strava_client_id"),
        athlete_name: t.map(|t| t.athlete_name),
        auto_upload: meta(app, "strava_auto_upload").as_deref() == Some("1"),
        api_base: api_base(app),
    }
}

pub fn save_client(app: &AppHandle, id: &str, secret: &str) -> Result<()> {
    let id = id.trim();
    let secret = secret.trim();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        bail!("The Client ID is the number shown on strava.com/settings/api");
    }
    if secret.len() < 20 {
        bail!("That Client Secret looks too short");
    }
    set_meta(app, "strava_client_id", id)?;
    set_meta(app, "strava_client_secret", secret)?;
    Ok(())
}

pub fn set_auto(app: &AppHandle, on: bool) -> Result<()> {
    set_meta(app, "strava_auto_upload", if on { "1" } else { "0" })
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(60)).user_agent("CoreKick/0.1").build().expect("http client")
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    #[serde(default)]
    athlete: Option<AthleteSummary>,
}

#[derive(Deserialize)]
struct AthleteSummary {
    id: u64,
    #[serde(default)]
    firstname: String,
    #[serde(default)]
    lastname: String,
}

fn query_param(q: &str, key: &str) -> Option<String> {
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| v.replace('+', " ")).map(|v| percent_decode(&v))
    })
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

const DONE_PAGE: &str = "<!doctype html><html><body style=\"font-family:-apple-system,sans-serif;background:#0a0d13;color:#e9edf4;display:grid;place-items:center;height:100vh;margin:0\"><div style=\"text-align:center\"><h1 style=\"color:#7cffb2\">Connected to Strava</h1><p>You can close this tab and return to CoreKick.</p></div></body></html>";

/// Run the OAuth flow. Opens the system browser and waits up to 5 minutes.
pub async fn connect(app: AppHandle) -> Result<StravaStatus> {
    let (client_id, client_secret) = client(&app).ok_or_else(|| anyhow!("Add your Strava app's Client ID and Secret first"))?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.context("Could not open a local port for the Strava callback")?;
    let port = listener.local_addr()?.port();
    let state: String = uuid::Uuid::new_v4().simple().to_string();
    let redirect = format!("http://127.0.0.1:{port}/strava/callback");
    let url = format!(
        "{OAUTH_BASE}/authorize?client_id={client_id}&response_type=code&redirect_uri={}&approval_prompt=auto&scope=read,activity:write&state={state}",
        redirect.replace(':', "%3A").replace('/', "%2F")
    );
    app.opener().open_url(&url, None::<&str>).map_err(|e| anyhow!("Could not open the browser: {e}"))?;

    let code = tokio::time::timeout(Duration::from_secs(300), async {
        loop {
            let (mut sock, _) = listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req.split_whitespace().nth(1).unwrap_or("").to_string();
            if !path.starts_with("/strava/callback") {
                let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                continue;
            }
            let q = path.split_once('?').map(|x| x.1).unwrap_or("");
            let (body, result) = if let Some(err) = query_param(q, "error") {
                (format!("<h1>Strava authorisation failed</h1><p>{err}</p>"), Err(anyhow!("Strava said: {err}")))
            } else if query_param(q, "state").as_deref() != Some(state.as_str()) {
                ("<h1>Invalid state</h1>".to_string(), Err(anyhow!("OAuth state mismatch – try again")))
            } else if let Some(code) = query_param(q, "code") {
                let scope = query_param(q, "scope").unwrap_or_default();
                if !scope.contains("activity:write") {
                    ("<h1>Uploads not allowed</h1><p>Please tick “Upload your activities” and try again.</p>".to_string(), Err(anyhow!("CoreKick needs the “Upload your activities” permission")))
                } else {
                    (DONE_PAGE.to_string(), Ok(code))
                }
            } else {
                ("<h1>Missing code</h1>".to_string(), Err(anyhow!("Strava returned no authorisation code")))
            };
            let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = sock.shutdown().await;
            return result;
        }
    })
    .await
    .map_err(|_| anyhow!("Timed out waiting for Strava (5 minutes)"))??;

    let resp = http()
        .post(format!("{OAUTH_BASE}/token"))
        .form(&[("client_id", client_id.as_str()), ("client_secret", client_secret.as_str()), ("code", code.as_str()), ("grant_type", "authorization_code")])
        .send()
        .await
        .context("Token request failed")?;
    if !resp.status().is_success() {
        bail!("Strava rejected the token exchange ({}): {}", resp.status(), resp.text().await.unwrap_or_default());
    }
    let t: TokenResponse = resp.json().await?;
    let athlete = t.athlete.ok_or_else(|| anyhow!("Strava returned no athlete"))?;
    let tok = Tokens {
        access_token: t.access_token,
        refresh_token: t.refresh_token,
        expires_at: t.expires_at,
        athlete_id: athlete.id,
        athlete_name: format!("{} {}", athlete.firstname, athlete.lastname).trim().to_string(),
    };
    set_meta(&app, "strava_tokens", &serde_json::to_string(&tok)?)?;
    Ok(status(&app))
}

async fn fresh_token(app: &AppHandle) -> Result<String> {
    let mut t = tokens(app).ok_or_else(|| anyhow!("Not connected to Strava"))?;
    if t.expires_at - chrono::Utc::now().timestamp() > 120 {
        return Ok(t.access_token);
    }
    let (id, secret) = client(app).ok_or_else(|| anyhow!("Strava app credentials missing"))?;
    let resp = http()
        .post(format!("{OAUTH_BASE}/token"))
        .form(&[("client_id", id.as_str()), ("client_secret", secret.as_str()), ("refresh_token", t.refresh_token.as_str()), ("grant_type", "refresh_token")])
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!("Strava token refresh failed ({}) – reconnect in Settings", resp.status());
    }
    let r: TokenResponse = resp.json().await?;
    t.access_token = r.access_token;
    t.refresh_token = r.refresh_token;
    t.expires_at = r.expires_at;
    set_meta(app, "strava_tokens", &serde_json::to_string(&t)?)?;
    Ok(t.access_token)
}

pub async fn disconnect(app: AppHandle) -> Result<()> {
    if let Some(t) = tokens(&app) {
        // Best effort: revoke the token on Strava's side.
        let _ = http().post(format!("{OAUTH_BASE}/revoke")).form(&[("token", t.access_token.as_str())]).send().await;
    }
    set_meta(&app, "strava_tokens", "")?;
    Ok(())
}

#[derive(Deserialize, Debug)]
struct UploadStatus {
    id: u64,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    activity_id: Option<u64>,
}

fn duplicate_id(err: &str) -> Option<u64> {
    let i = err.find("activities/")? + "activities/".len();
    err[i..].chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()
}

fn emit(app: &AppHandle, ev: UploadEvent) {
    let _ = app.emit("strava://upload", ev);
}

/// Upload a saved ride. Returns the Strava activity id.
pub async fn upload(app: AppHandle, ride_id: String) -> Result<u64> {
    let rec = app.state::<AppState>().store.lock().unwrap().ride(&ride_id)?.ok_or_else(|| anyhow!("Ride not found"))?;
    if let Some(id) = rec.summary.strava_activity_id {
        return Ok(id);
    }
    emit(&app, UploadEvent { ride_id: ride_id.clone(), status: "uploading".into(), activity_id: None, error: None });
    let result = async {
        let token = fresh_token(&app).await?;
        let base = api_base(&app);
        let fit = corekick_core::fit::to_fit(&rec);
        let s = &rec.summary;
        let mut desc = format!(
            "{} · {:.0} W avg / {:.0} W NP · {:.0} kJ · TSS {:.0}",
            match s.kind {
                corekick_core::model::RideKind::Route => "Route",
                corekick_core::model::RideKind::Workout => "Workout",
                corekick_core::model::RideKind::Test => "Fitness test",
                corekick_core::model::RideKind::Free => "Free ride",
            },
            s.avg_power,
            s.np,
            s.work_kj,
            s.tss
        );
        if let Some(t) = &s.test_result {
            desc.push_str(&format!(" · FTP estimate {:.0} W", t.ftp));
        }
        if !s.notes.trim().is_empty() {
            desc = format!("{}\n\n{desc}", s.notes.trim());
        }
        desc.push_str("\nRecorded with CoreKick");
        let form = reqwest::multipart::Form::new()
            .part("file", reqwest::multipart::Part::bytes(fit).file_name(format!("corekick-{}.fit", s.id)).mime_str("application/octet-stream")?)
            .text("data_type", "fit")
            .text("name", s.name.clone())
            .text("description", desc)
            .text("trainer", "1")
            .text("external_id", format!("corekick-{}.fit", s.id));
        let resp = http().post(format!("{base}/uploads")).bearer_auth(&token).multipart(form).send().await.context("Upload request failed")?;
        let code = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !code.is_success() {
            if code.as_u16() == 429 {
                bail!("Strava rate limit reached – try again in 15 minutes");
            }
            if code.as_u16() == 401 || code.as_u16() == 403 {
                bail!("Strava refused the upload ({code}). Your API app may need a Strava subscription, or reconnect in Settings.");
            }
            bail!("Strava upload failed ({code}): {text}");
        }
        let mut st: UploadStatus = serde_json::from_str(&text).with_context(|| format!("Unexpected Strava response: {text}"))?;
        emit(&app, UploadEvent { ride_id: ride_id.clone(), status: "processing".into(), activity_id: None, error: None });
        for _ in 0..90 {
            if let Some(id) = st.activity_id {
                return Ok(id);
            }
            if let Some(e) = st.error.clone().filter(|e| !e.is_empty()) {
                if let Some(id) = duplicate_id(&e) {
                    return Ok(id);
                }
                bail!("Strava couldn't process the file: {e}");
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
            st = http().get(format!("{base}/uploads/{}", st.id)).bearer_auth(&token).send().await?.json().await?;
            let _ = &st.status;
        }
        bail!("Strava is still processing – check your Strava feed shortly")
    }
    .await;

    match result {
        Ok(activity_id) => {
            // Make sure it's filed as a Virtual Ride on the trainer.
            if let Ok(token) = fresh_token(&app).await {
                let _ = http()
                    .put(format!("{}/activities/{activity_id}", api_base(&app)))
                    .bearer_auth(token)
                    .json(&serde_json::json!({ "sport_type": "VirtualRide", "trainer": true }))
                    .send()
                    .await;
            }
            {
                let state = app.state::<AppState>();
                let store = state.store.lock().unwrap();
                if let Ok(Some(mut r)) = store.ride(&ride_id) {
                    r.summary.strava_activity_id = Some(activity_id);
                    let _ = store.update_summary(&r.summary);
                }
            }
            emit(&app, UploadEvent { ride_id, status: "done".into(), activity_id: Some(activity_id), error: None });
            Ok(activity_id)
        }
        Err(e) => {
            emit(&app, UploadEvent { ride_id, status: "error".into(), activity_id: None, error: Some(format!("{e:#}")) });
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_helpers() {
        assert_eq!(query_param("state=abc&code=12%2F3&scope=read,activity:write", "code").as_deref(), Some("12/3"));
        assert_eq!(query_param("a=1", "b"), None);
        assert_eq!(duplicate_id("duplicate of <a href='/activities/123456789' target='_blank'>Morning Ride</a>"), Some(123456789));
        assert_eq!(duplicate_id("Improperly formatted data."), None);
    }
}
