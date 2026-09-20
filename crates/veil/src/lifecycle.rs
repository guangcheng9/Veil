use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, watch};
use veil_engine::mapping::MappingStore;

use crate::admin::TrafficLog;
use crate::config::Config;
use crate::process::{self, RuntimeMetadata};
use crate::proxy::{router, AppState, ProtocolFamily, UpstreamConfig};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyState {
    Starting,
    Running,
    Stopping,
    #[default]
    Stopped,
    Degraded,
}

impl ProxyState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::Degraded => "degraded",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlPlaneState {
    #[default]
    Running,
    Stopping,
}

impl ControlPlaneState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopping => "stopping",
        }
    }
}

impl From<ControlPlaneState> for &'static str {
    fn from(s: ControlPlaneState) -> Self {
        s.as_str()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LifecycleSnapshot {
    pub instance_id: String,
    pub pid: u32,
    pub started_at: String,
    pub control_plane: ControlPlaneState,
    pub proxy_state: ProxyState,
    pub listeners: Vec<u16>,
    pub last_lifecycle_error: Option<String>,
    /// True when upstream/port config changed and the data plane still holds
    /// the old reqwest clients / listeners — a restart is required to apply.
    pub pending_config_change: bool,
}

enum Command {
    Start(oneshot::Sender<Result<LifecycleSnapshot, String>>),
    Stop(oneshot::Sender<Result<LifecycleSnapshot, String>>),
    Restart(oneshot::Sender<Result<LifecycleSnapshot, String>>),
    Exit,
    MarkConfigChanged,
}

#[derive(Clone)]
pub struct LifecycleHandle {
    commands: mpsc::Sender<Command>,
    status: watch::Receiver<LifecycleSnapshot>,
}

impl LifecycleHandle {
    pub fn snapshot(&self) -> LifecycleSnapshot {
        self.status.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<LifecycleSnapshot> {
        self.status.clone()
    }

    /// Mark that upstream/port config changed so the data plane is stale; the
    /// UI uses this to badge the "restart forwarding" button. Cleared on the
    /// next start/restart.
    pub fn mark_config_changed(&self) {
        let _ = self.commands.try_send(Command::MarkConfigChanged);
    }

    pub async fn start(&self) -> Result<LifecycleSnapshot, String> {
        self.request(Command::Start).await
    }

    pub async fn stop(&self) -> Result<LifecycleSnapshot, String> {
        self.request(Command::Stop).await
    }

    pub async fn restart(&self) -> Result<LifecycleSnapshot, String> {
        self.request(Command::Restart).await
    }

    pub fn exit(&self) -> Result<(), String> {
        self.commands
            .try_send(Command::Exit)
            .map_err(|e| format!("lifecycle controller unavailable: {e}"))
    }

    async fn request(
        &self,
        make: impl FnOnce(oneshot::Sender<Result<LifecycleSnapshot, String>>) -> Command,
    ) -> Result<LifecycleSnapshot, String> {
        let (tx, rx) = oneshot::channel();
        self.commands
            .send(make(tx))
            .await
            .map_err(|_| "lifecycle controller unavailable".to_string())?;
        rx.await
            .map_err(|_| "lifecycle controller stopped before replying".to_string())?
    }
}

struct DataPlane {
    shutdown: watch::Sender<bool>,
    tasks: Vec<tokio::task::JoinHandle<Result<(), std::io::Error>>>,
}

pub struct LifecycleParams {
    pub data_dir: PathBuf,
    pub config: Arc<Mutex<Config>>,
    pub store: Arc<dyn MappingStore + Send + Sync>,
    pub traffic: TrafficLog,
    pub instance_id: String,
    pub started_at: String,
    pub management_port: u16,
    pub exit_tx: watch::Sender<bool>,
}

pub fn spawn(params: LifecycleParams) -> LifecycleHandle {
    let initial = LifecycleSnapshot {
        instance_id: params.instance_id.clone(),
        pid: std::process::id(),
        started_at: params.started_at.clone(),
        control_plane: ControlPlaneState::Running,
        proxy_state: ProxyState::Stopped,
        listeners: Vec::new(),
        last_lifecycle_error: None,
        pending_config_change: false,
    };
    let (status_tx, status_rx) = watch::channel(initial);
    let (command_tx, command_rx) = mpsc::channel(16);
    tokio::spawn(run(params, command_rx, status_tx));
    LifecycleHandle {
        commands: command_tx,
        status: status_rx,
    }
}

async fn run(
    params: LifecycleParams,
    mut commands: mpsc::Receiver<Command>,
    status_tx: watch::Sender<LifecycleSnapshot>,
) {
    let mut plane: Option<DataPlane> = None;
    while let Some(command) = commands.recv().await {
        match command {
            Command::Start(reply) => {
                let result = start_plane(&params, &status_tx, &mut plane).await;
                let _ = reply.send(result);
            }
            Command::Stop(reply) => {
                let result = stop_plane(&params, &status_tx, &mut plane).await;
                let _ = reply.send(result);
            }
            Command::Restart(reply) => {
                let result = async {
                    stop_plane(&params, &status_tx, &mut plane).await?;
                    start_plane(&params, &status_tx, &mut plane).await
                }
                .await;
                let _ = reply.send(result);
            }
            Command::Exit => {
                // Announce control-plane shutdown *before* tearing things down so
                // the status API can surface "stopping" instead of a hard cut.
                publish_control_plane(&params, &status_tx, ControlPlaneState::Stopping);
                let _ = stop_plane(&params, &status_tx, &mut plane).await;
                let _ = params.exit_tx.send(true);
                break;
            }
            Command::MarkConfigChanged => {
                let mut snap = status_tx.borrow().clone();
                snap.pending_config_change = true;
                status_tx.send_replace(snap);
            }
        }
    }
}

async fn start_plane(
    params: &LifecycleParams,
    status_tx: &watch::Sender<LifecycleSnapshot>,
    plane: &mut Option<DataPlane>,
) -> Result<LifecycleSnapshot, String> {
    if plane.is_some() {
        return Ok(status_tx.borrow().clone());
    }
    publish(
        params,
        status_tx,
        ControlPlaneState::Running,
        ProxyState::Starting,
        Vec::new(),
        None,
    );
    let cfg = match Config::load(&params.data_dir) {
        Ok(cfg) => {
            *params.config.lock().unwrap_or_else(|e| e.into_inner()) = cfg.clone();
            cfg
        }
        Err(e) => return degraded(params, status_tx, e.to_string()),
    };
    if let Err(e) = cfg.validate() {
        return degraded(params, status_tx, e.to_string());
    }

    let limit = (cfg.request_body_limit_mib.saturating_mul(1024 * 1024)) as usize;
    let mut base = match AppState::new(params.store.clone(), UpstreamConfig::from(&cfg), limit) {
        Ok(state) => state.with_traffic(params.traffic.clone()),
        Err(e) => return degraded(params, status_tx, e.to_string()),
    };
    base.alias_hint = cfg.alias_hint;

    let mut bound = Vec::new();
    let main = bind(&cfg.bind, cfg.proxy_port).await.map_err(|e| {
        format!(
            "cannot bind data-plane port {}: {e}; {}",
            cfg.proxy_port,
            process::describe_port_owner(cfg.proxy_port)
        )
    });
    let main = match main {
        Ok(listener) => listener,
        Err(error) => return degraded(params, status_tx, error),
    };
    bound.push((cfg.proxy_port, main, base.clone()));

    for route_cfg in &cfg.routes {
        let listener = match bind(&cfg.bind, route_cfg.port).await {
            Ok(listener) => listener,
            Err(e) => {
                drop(bound);
                return degraded(
                    params,
                    status_tx,
                    format!(
                        "cannot bind route {} port {}: {e}; {}",
                        route_cfg.id,
                        route_cfg.port,
                        process::describe_port_owner(route_cfg.port)
                    ),
                );
            }
        };
        let mut state = base.clone();
        state.upstream = UpstreamConfig::for_route(&cfg, route_cfg);
        state.default_family = match route_cfg.kind.as_str() {
            "anthropic" => Some(ProtocolFamily::Anthropic),
            "openai" => Some(ProtocolFamily::OpenAiCompletions),
            _ => None,
        };
        bound.push((route_cfg.port, listener, state));
    }

    let listeners: Vec<u16> = bound.iter().map(|(port, _, _)| *port).collect();
    let (shutdown_tx, _) = watch::channel(false);
    let mut tasks = Vec::with_capacity(bound.len());
    for (_, listener, state) in bound {
        let mut shutdown = shutdown_tx.subscribe();
        tasks.push(tokio::spawn(async move {
            axum::serve(listener, router(state))
                .with_graceful_shutdown(async move {
                    while !*shutdown.borrow() {
                        if shutdown.changed().await.is_err() {
                            break;
                        }
                    }
                })
                .await
        }));
    }
    *plane = Some(DataPlane {
        shutdown: shutdown_tx,
        tasks,
    });
    Ok(publish(
        params,
        status_tx,
        ControlPlaneState::Running,
        ProxyState::Running,
        listeners,
        None,
    ))
}

async fn stop_plane(
    params: &LifecycleParams,
    status_tx: &watch::Sender<LifecycleSnapshot>,
    plane: &mut Option<DataPlane>,
) -> Result<LifecycleSnapshot, String> {
    let Some(active) = plane.take() else {
        return Ok(publish(
            params,
            status_tx,
            ControlPlaneState::Running,
            ProxyState::Stopped,
            Vec::new(),
            None,
        ));
    };
    let old = status_tx.borrow().clone();
    publish(
        params,
        status_tx,
        ControlPlaneState::Running,
        ProxyState::Stopping,
        old.listeners,
        None,
    );
    let _ = active.shutdown.send(true);
    let timeout_secs = params
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .graceful_shutdown_timeout_secs
        .max(1);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    let mut tasks = active.tasks;
    let mut failure = None;
    for task in &mut tasks {
        match tokio::time::timeout_at(deadline, task).await {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(e))) => {
                failure = Some(format!("data-plane server failed: {e}"));
                break;
            }
            Ok(Err(e)) => {
                failure = Some(format!("data-plane task failed: {e}"));
                break;
            }
            Err(_) => {
                failure = Some(format!(
                    "data-plane graceful shutdown timed out after {timeout_secs} seconds"
                ));
                break;
            }
        }
    }
    if let Some(error) = failure {
        for task in &tasks {
            task.abort();
        }
        for task in tasks {
            let _ = task.await;
        }
        degraded(params, status_tx, error)
    } else {
        Ok(publish(
            params,
            status_tx,
            ControlPlaneState::Running,
            ProxyState::Stopped,
            Vec::new(),
            None,
        ))
    }
}

async fn bind(host: &str, port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(format!("{host}:{port}")).await
}

fn degraded(
    params: &LifecycleParams,
    status_tx: &watch::Sender<LifecycleSnapshot>,
    error: String,
) -> Result<LifecycleSnapshot, String> {
    publish(
        params,
        status_tx,
        ControlPlaneState::Running,
        ProxyState::Degraded,
        Vec::new(),
        Some(error.clone()),
    );
    Err(error)
}

fn publish(
    params: &LifecycleParams,
    status_tx: &watch::Sender<LifecycleSnapshot>,
    control_plane: ControlPlaneState,
    proxy_state: ProxyState,
    listeners: Vec<u16>,
    error: Option<String>,
) -> LifecycleSnapshot {
    publish_full(
        params,
        status_tx,
        control_plane,
        proxy_state,
        listeners,
        error,
        false,
    )
}

fn publish_full(
    params: &LifecycleParams,
    status_tx: &watch::Sender<LifecycleSnapshot>,
    control_plane: ControlPlaneState,
    proxy_state: ProxyState,
    mut listeners: Vec<u16>,
    error: Option<String>,
    pending_config_change: bool,
) -> LifecycleSnapshot {
    listeners.sort_unstable();
    let snapshot = LifecycleSnapshot {
        instance_id: params.instance_id.clone(),
        pid: std::process::id(),
        started_at: params.started_at.clone(),
        control_plane,
        proxy_state,
        listeners: listeners.clone(),
        last_lifecycle_error: error.clone(),
        pending_config_change,
    };
    let mut all_ports = vec![params.management_port];
    all_ports.extend(listeners);
    all_ports.sort_unstable();
    all_ports.dedup();
    let metadata = RuntimeMetadata {
        pid: snapshot.pid,
        process_started_at: process::current_process_started_at(),
        instance_id: snapshot.instance_id.clone(),
        exe_path: process::current_exe(),
        management_port: params.management_port,
        listener_ports: all_ports,
        proxy_state: proxy_state.as_str().into(),
        started_at: snapshot.started_at.clone(),
        last_lifecycle_error: error,
    };
    if let Err(e) = process::write_runtime(&params.data_dir, &metadata) {
        eprintln!("cannot update runtime metadata: {e}");
    }
    status_tx.send_replace(snapshot.clone());
    snapshot
}

/// Update only the control-plane field of the live snapshot (used on exit so
/// clients see "stopping" before the process tears down).
fn publish_control_plane(
    params: &LifecycleParams,
    status_tx: &watch::Sender<LifecycleSnapshot>,
    state: ControlPlaneState,
) {
    let mut snap = status_tx.borrow().clone();
    snap.control_plane = state;
    let metadata = RuntimeMetadata {
        pid: snap.pid,
        process_started_at: process::current_process_started_at(),
        instance_id: snap.instance_id.clone(),
        exe_path: process::current_exe(),
        management_port: params.management_port,
        listener_ports: snap.listeners.clone(),
        proxy_state: snap.proxy_state.as_str().into(),
        started_at: snap.started_at.clone(),
        last_lifecycle_error: snap.last_lifecycle_error.clone(),
    };
    if let Err(e) = process::write_runtime(&params.data_dir, &metadata) {
        eprintln!("cannot update runtime metadata: {e}");
    }
    status_tx.send_replace(snap);
}

#[cfg(test)]
mod tests {
    use super::*;
    use veil_engine::mapping::MemoryStore;

    async fn controller(
        cfg: Config,
    ) -> (LifecycleHandle, tempfile::TempDir, watch::Receiver<bool>) {
        let dir = tempfile::tempdir().unwrap();
        cfg.save(dir.path()).unwrap();
        let (exit_tx, exit_rx) = watch::channel(false);
        let handle = spawn(LifecycleParams {
            data_dir: dir.path().to_path_buf(),
            config: Arc::new(Mutex::new(cfg)),
            store: Arc::new(MemoryStore::new()),
            traffic: TrafficLog::default(),
            instance_id: ulid::Ulid::new().to_string(),
            started_at: process::now_rfc3339(),
            management_port: 18788,
            exit_tx,
        });
        (handle, dir, exit_rx)
    }

    #[tokio::test]
    async fn start_stop_start_is_repeatable() {
        let mut cfg = Config::default();
        let reserve = TcpListener::bind("127.0.0.1:0").await.unwrap();
        cfg.proxy_port = reserve.local_addr().unwrap().port();
        cfg.management_port = 1;
        drop(reserve);
        let (handle, _dir, _exit) = controller(cfg).await;
        assert_eq!(
            handle.start().await.unwrap().proxy_state,
            ProxyState::Running
        );
        assert_eq!(
            handle.stop().await.unwrap().proxy_state,
            ProxyState::Stopped
        );
        assert_eq!(
            handle.start().await.unwrap().proxy_state,
            ProxyState::Running
        );
        handle.stop().await.unwrap();
    }

    #[tokio::test]
    async fn bind_failure_rolls_back_and_degrades() {
        let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut cfg = Config::default();
        cfg.proxy_port = occupied.local_addr().unwrap().port();
        cfg.management_port = 1;
        let (handle, _dir, _exit) = controller(cfg).await;
        assert!(handle.start().await.is_err());
        assert_eq!(handle.snapshot().proxy_state, ProxyState::Degraded);
        assert!(handle.snapshot().listeners.is_empty());
    }
}
