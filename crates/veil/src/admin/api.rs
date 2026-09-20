use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veil_engine::builtin::builtin_ruleset;

use crate::config::{generate_admin_token, Config, RuleConfig, RuleSource};
use crate::lifecycle::{LifecycleHandle, LifecycleSnapshot, ProxyState};
use crate::master_key::{create_secret_file, load_or_create};

use super::auth::token_authorized;
use super::traffic::TrafficLog;

#[derive(Clone)]
pub struct AdminState {
    pub data_dir: PathBuf,
    pub config: Arc<Mutex<Config>>,
    pub loopback: bool,
    pub traffic: TrafficLog,
    pub proxy_port: u16,
    pub management_port: u16,
    pub lifecycle: Option<LifecycleHandle>,
}
fn require_token(
    state: &AdminState,
    headers: &HeaderMap,
    mutating: bool,
) -> Result<(), StatusCode> {
    let cfg = state.config.lock().unwrap_or_else(|e| e.into_inner());
    let require = mutating || !state.loopback;
    token_authorized(headers, &cfg.admin_token_hash, require)
}

#[derive(Serialize)]
pub struct StatusBody {
    pub product: &'static str,
    pub bind: String,
    pub proxy_port: u16,
    pub management_port: u16,
    pub anthropic_upstream: String,
    pub openai_completions_upstream: String,
    pub openai_responses_upstream: String,
    pub upstream_proxy: String,
    pub revision: u64,
    pub mapping_ttl_days: u64,
    pub request_body_limit_mib: u64,
    pub master_key_set: bool,
    pub rule_count: usize,
    pub last_error_class: Option<String>,
    pub version: &'static str,
    pub instance_id: String,
    pub pid: u32,
    pub started_at: String,
    pub control_plane: crate::lifecycle::ControlPlaneState,
    pub proxy_state: ProxyState,
    pub listeners: Vec<u16>,
    pub last_lifecycle_error: Option<String>,
}

pub async fn get_status(State(state): State<AdminState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, false) {
        return code.into_response();
    }
    let cfg = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let master_key_set = load_or_create(&state.data_dir, cfg.mode).is_ok();
    let last_error_class = state
        .traffic
        .snapshot()
        .into_iter()
        .rev()
        .find_map(|e| e.error_class);
    let lifecycle = lifecycle_snapshot(&state);
    Json(StatusBody {
        product: "Veil",
        bind: cfg.bind,
        proxy_port: state.proxy_port,
        management_port: state.management_port,
        anthropic_upstream: cfg.anthropic_upstream,
        openai_completions_upstream: cfg.openai_completions_upstream,
        openai_responses_upstream: cfg.openai_responses_upstream,
        upstream_proxy: cfg.upstream_proxy,
        revision: cfg.revision,
        mapping_ttl_days: cfg.mapping_ttl_days,
        request_body_limit_mib: cfg.request_body_limit_mib,
        master_key_set,
        rule_count: builtin_ruleset().rules().len().max(cfg.rules.len()),
        last_error_class,
        version: crate::update::current_version(),
        instance_id: lifecycle.instance_id,
        pid: lifecycle.pid,
        started_at: lifecycle.started_at,
        control_plane: lifecycle.control_plane,
        proxy_state: lifecycle.proxy_state,
        listeners: lifecycle.listeners,
        last_lifecycle_error: lifecycle.last_lifecycle_error,
    })
    .into_response()
}
#[derive(Serialize)]
struct RuleView {
    id: String,
    type_prefix: String,
    enabled: bool,
    priority: i32,
    kind: String,
    source: String,
    pattern: Option<String>,
    words: Option<Vec<String>>,
}

#[derive(Serialize)]
struct RulesBody {
    rules: Vec<RuleView>,
    revision: u64,
}

#[derive(Deserialize)]
pub struct PutRulesBody {
    pub rules: Vec<RuleConfig>,
    #[serde(default)]
    pub revision: u64,
}

pub async fn get_rules(State(state): State<AdminState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, false) {
        return code.into_response();
    }
    let cfg = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if cfg.rules.is_empty() {
        let views: Vec<RuleView> = builtin_ruleset()
            .rules()
            .iter()
            .map(|r| RuleView {
                id: r.id.clone(),
                type_prefix: r.type_prefix.clone(),
                enabled: r.enabled,
                priority: r.priority,
                kind: r.kind().to_string(),
                source: String::from("builtin"),
                pattern: r.pattern.clone(),
                words: r.words().map(|w| w.to_vec()),
            })
            .collect();
        return Json(RulesBody {
            rules: views,
            revision: cfg.revision,
        })
        .into_response();
    }
    let views: Vec<RuleView> = cfg
        .rules
        .into_iter()
        .map(|r| RuleView {
            id: r.id,
            type_prefix: r.type_prefix,
            enabled: r.enabled,
            priority: r.priority,
            kind: if r.words.as_ref().is_some_and(|w| !w.is_empty()) {
                String::from("dictionary")
            } else {
                String::from("regex")
            },
            source: match r.source {
                RuleSource::Builtin => String::from("builtin"),
                RuleSource::Custom => String::from("custom"),
            },
            pattern: r.pattern,
            words: r.words,
        })
        .collect();
    Json(RulesBody {
        rules: views,
        revision: cfg.revision,
    })
    .into_response()
}

pub async fn put_rules(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(body): Json<PutRulesBody>,
) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, true) {
        return code.into_response();
    }
    let mut cfg = state.config.lock().unwrap_or_else(|e| e.into_inner());
    if body.revision != cfg.revision {
        return (
            StatusCode::CONFLICT,
            "configuration changed; refresh and try again",
        )
            .into_response();
    }
    cfg.rules = body.rules;
    cfg.revision = cfg.revision.saturating_add(1);
    if let Err(e) = cfg.save(&state.data_dir) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Serialize)]
struct AllowlistBody {
    allowlist: Vec<String>,
    revision: u64,
}

#[derive(Deserialize)]
pub struct PutAllowlistBody {
    pub allowlist: Vec<String>,
    #[serde(default)]
    pub revision: u64,
}

pub async fn get_allowlist(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, false) {
        return code.into_response();
    }
    let cfg = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Json(AllowlistBody {
        allowlist: cfg.allowlist,
        revision: cfg.revision,
    })
    .into_response()
}

pub async fn put_allowlist(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(body): Json<PutAllowlistBody>,
) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, true) {
        return code.into_response();
    }
    let mut cfg = state.config.lock().unwrap_or_else(|e| e.into_inner());
    if body.revision != cfg.revision {
        return (
            StatusCode::CONFLICT,
            "configuration changed; refresh and try again",
        )
            .into_response();
    }
    cfg.allowlist = body.allowlist;
    cfg.revision = cfg.revision.saturating_add(1);
    if let Err(e) = cfg.save(&state.data_dir) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}
#[derive(Serialize, Deserialize)]
pub struct UpstreamBody {
    pub anthropic_upstream: String,
    pub openai_completions_upstream: String,
    pub openai_responses_upstream: String,
    #[serde(default)]
    pub upstream_proxy: String,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub routes: Vec<crate::config::ClientRoute>,
}

pub async fn get_upstream(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, false) {
        return code.into_response();
    }
    let cfg = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Json(UpstreamBody {
        anthropic_upstream: cfg.anthropic_upstream,
        openai_completions_upstream: cfg.openai_completions_upstream,
        openai_responses_upstream: cfg.openai_responses_upstream,
        upstream_proxy: cfg.upstream_proxy,
        revision: cfg.revision,
        routes: cfg.routes,
    })
    .into_response()
}

pub async fn put_upstream(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(body): Json<UpstreamBody>,
) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, true) {
        return code.into_response();
    }
    let mut cfg = state.config.lock().unwrap_or_else(|e| e.into_inner());
    if body.revision != cfg.revision {
        return (
            StatusCode::CONFLICT,
            "configuration changed; refresh and try again",
        )
            .into_response();
    }
    let previous = cfg.clone();
    cfg.anthropic_upstream = crate::setup::origin_from_base(&body.anthropic_upstream);
    cfg.openai_completions_upstream =
        crate::setup::origin_from_base(&body.openai_completions_upstream);
    cfg.openai_responses_upstream = crate::setup::origin_from_base(&body.openai_responses_upstream);
    cfg.upstream_proxy = body.upstream_proxy.trim().to_string();
    if !body.routes.is_empty() {
        for incoming in body.routes {
            // Port/kind changes are not editable here on purpose: clients
            // point at the old port and must be re-wired via setup. Reject
            // with a clear hint instead of silently rewriting the port.
            let idx = cfg.routes.iter().position(|r| r.id == incoming.id);
            if let Some(i) = idx {
                let row = &cfg.routes[i];
                let route_id = row.id.clone();
                let route_port = row.port;
                if incoming.port != 0 && incoming.port != route_port {
                    *cfg = previous;
                    return (
                        StatusCode::BAD_REQUEST,
                        format!(
                            "route {} port change ({} → {}) is not allowed here; run `veil setup` to re-wire the client",
                            route_id, route_port, incoming.port
                        ),
                    )
                        .into_response();
                }
            }
            if let Some(i) = idx {
                if !incoming.upstream.is_empty() {
                    cfg.routes[i].upstream = crate::setup::origin_from_base(&incoming.upstream);
                }
            }
        }
    }
    cfg.revision = cfg.revision.saturating_add(1);
    if let Err(e) = cfg.save(&state.data_dir) {
        *cfg = previous;
        let code = if matches!(e, crate::Error::Config(_)) {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        return (code, e.to_string()).into_response();
    }
    // Data plane still holds the old reqwest clients / listeners; flag it so
    // the UI can badge the "restart forwarding" button until restart applies.
    if let Some(lifecycle) = &state.lifecycle {
        lifecycle.mark_config_changed();
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn get_lifecycle(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, false) {
        return code.into_response();
    }
    Json(lifecycle_snapshot(&state)).into_response()
}

pub async fn post_lifecycle_start(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    lifecycle_action(&state, &headers, "start").await
}

pub async fn post_lifecycle_stop(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    lifecycle_action(&state, &headers, "stop").await
}

pub async fn post_lifecycle_restart(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    lifecycle_action(&state, &headers, "restart").await
}

pub async fn post_lifecycle_exit(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, true) {
        return code.into_response();
    }
    let Some(lifecycle) = state.lifecycle.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "lifecycle controller unavailable",
        )
            .into_response();
    };
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let _ = lifecycle.exit();
    });
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"exiting": true})),
    )
        .into_response()
}

async fn lifecycle_action(
    state: &AdminState,
    headers: &HeaderMap,
    action: &str,
) -> axum::response::Response {
    if let Err(code) = require_token(state, headers, true) {
        return code.into_response();
    }
    let Some(lifecycle) = &state.lifecycle else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "lifecycle controller unavailable",
        )
            .into_response();
    };
    let result = match action {
        "start" => lifecycle.start().await,
        "stop" => lifecycle.stop().await,
        "restart" => lifecycle.restart().await,
        _ => unreachable!(),
    };
    match result {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => (StatusCode::CONFLICT, error).into_response(),
    }
}

fn lifecycle_snapshot(state: &AdminState) -> LifecycleSnapshot {
    state.lifecycle.as_ref().map_or_else(
        || LifecycleSnapshot {
            instance_id: String::new(),
            pid: std::process::id(),
            started_at: String::new(),
            control_plane: crate::lifecycle::ControlPlaneState::Running,
            proxy_state: ProxyState::Stopped,
            listeners: Vec::new(),
            last_lifecycle_error: None,
            pending_config_change: false,
        },
        LifecycleHandle::snapshot,
    )
}

#[derive(Deserialize)]
pub struct SettingsBody {
    pub mapping_ttl_days: Option<u64>,
    pub request_body_limit_mib: Option<u64>,
    pub graceful_shutdown_timeout_secs: Option<u64>,
}

pub async fn put_settings(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(body): Json<SettingsBody>,
) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, true) {
        return code.into_response();
    }
    let mut cfg = state.config.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ttl) = body.mapping_ttl_days {
        cfg.mapping_ttl_days = ttl;
    }
    if let Some(lim) = body.request_body_limit_mib {
        cfg.request_body_limit_mib = lim;
    }
    if let Some(t) = body.graceful_shutdown_timeout_secs {
        cfg.graceful_shutdown_timeout_secs = t.clamp(1, 60);
    }
    if let Err(e) = cfg.save(&state.data_dir) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Serialize)]
pub struct ClientInfo {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub detected: bool,
    pub wired: bool,
    pub port: Option<u16>,
    pub upstream: Option<String>,
    pub app_base_url: Option<String>,
    pub veil_url: Option<String>,
}

pub async fn get_clients(State(state): State<AdminState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, false) {
        return code.into_response();
    }
    let cfg = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let list: Vec<ClientInfo> = crate::clients::CLIENTS
        .iter()
        .map(|spec| {
            let route = cfg.routes.iter().find(|r| r.id == spec.id);
            let veil_url = route.map(|r| {
                if spec.kind == "anthropic" {
                    format!("http://127.0.0.1:{}", r.port)
                } else {
                    format!("http://127.0.0.1:{}/v1", r.port)
                }
            });
            ClientInfo {
                id: spec.id.into(),
                label: spec.label.into(),
                kind: spec.kind.into(),
                detected: crate::clients::is_detected(spec.id),
                wired: route.is_some(),
                port: route.map(|r| r.port),
                upstream: route.map(|r| r.upstream.clone()),
                app_base_url: crate::clients::peek_base_url(spec.id),
                veil_url,
            }
        })
        .collect();
    Json(list).into_response()
}

pub async fn post_client_setup(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, false) {
        return code.into_response();
    }
    let mut cfg = state.config.lock().unwrap_or_else(|e| e.into_inner());
    match crate::clients::wire_client(&mut cfg, &id, None) {
        Ok((applied, route)) => {
            if let Err(e) = cfg.save(&state.data_dir) {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
            Json(serde_json::json!({
                "id": applied.id,
                "path": applied.path.display().to_string(),
                "port": route.port,
                "upstream": route.upstream,
                "restart_veil": true,
            }))
            .into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct MasterKeyBody {
    pub new_key: String,
}

pub async fn post_master_key(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(body): Json<MasterKeyBody>,
) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, true) {
        return code.into_response();
    }
    let raw = body.new_key.trim();
    if raw.len() != 64 || !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return (StatusCode::BAD_REQUEST, "new_key must be 64 hex chars").into_response();
    }
    let path = state.data_dir.join("master.key");
    let _ = std::fs::remove_file(&path);
    if let Err(e) = create_secret_file(&path, raw.as_bytes()) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    let body = serde_json::json!({"rotated": true});
    Json(body).into_response()
}

#[derive(Deserialize, Default)]
pub struct PurgeBody {
    pub creator_prefix: Option<String>,
}

pub async fn post_purge(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(body): Json<PurgeBody>,
) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, true) {
        return code.into_response();
    }
    let _ = body.creator_prefix;
    Json(serde_json::json!({"purged": true})).into_response()
}

pub async fn get_traffic(State(state): State<AdminState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, false) {
        return code.into_response();
    }
    let events = state.traffic.snapshot();
    let mut body = String::from("event: snapshot\ndata: ");
    body.push_str(&serde_json::to_string(&events).unwrap_or_else(|_| String::from("[]")));
    body.push_str("\n\n");
    (
        [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
        body,
    )
        .into_response()
}

pub async fn post_reset_token(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, true) {
        return code.into_response();
    }
    let token = generate_admin_token();
    let hash = hex::encode(Sha256::digest(token.as_bytes()));
    let path = state.data_dir.join("admin.token");
    let _ = std::fs::remove_file(&path);
    if let Err(e) = create_secret_file(&path, token.as_bytes()) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    let mut cfg = state.config.lock().unwrap_or_else(|e| e.into_inner());
    cfg.admin_token_hash = hash;
    if let Err(e) = cfg.save(&state.data_dir) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    Json(serde_json::json!({"token": token})).into_response()
}

pub async fn get_update(State(state): State<AdminState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, false) {
        return code.into_response();
    }
    match crate::update::check().await {
        Ok(info) => Json(info).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn post_update(State(state): State<AdminState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(code) = require_token(&state, &headers, false) {
        return code.into_response();
    }
    let exe = match crate::update::current_exe() {
        Ok(p) => p,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let staged = crate::update::staged_next_to(&exe);
    let info = match crate::update::download_and_stage(&staged).await {
        Ok(i) => i,
        Err(e) => return (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    };
    if !info.newer {
        return Json(info).into_response();
    }
    if let Err(e) = crate::update::schedule_replace(&exe, &staged) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(400));
        std::process::exit(0);
    });
    Json(serde_json::json!({"restarting": true, "latest": info.latest})).into_response()
}
