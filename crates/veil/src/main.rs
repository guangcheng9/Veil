//! CLI: `veil` (no args) starts and opens the UI.

use std::sync::Arc;
use std::sync::Mutex;

use std::io::{self, IsTerminal, Write};
use tokio::net::TcpListener;
use veil::admin::{self, AdminState, TrafficLog};
use veil::clients;
use veil::config::Config;
use veil::data_dir::default_data_dir;
use veil::lifecycle::{self, LifecycleParams};
use veil::logging;
use veil::master_key::load_or_create;
use veil::process::{self, RuntimeMetadata};
use veil::setup::{self, DEFAULT_PROXY_URL};
use veil::update;
use veil_store::SqliteStore;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("start");
    let foreground = args.iter().any(|a| a == "--foreground");
    if let Err(e) = dispatch(cmd, &args, foreground).await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

async fn dispatch(
    cmd: &str,
    args: &[String],
    foreground: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        "start" => cmd_start(foreground).await,
        "stop" => cmd_stop().await,
        "status" => cmd_status().await,
        "setup" => cmd_setup(args),
        "update" => cmd_update(args).await,
        "ui" => cmd_ui(true),
        "help" | "-h" | "--help" => {
            print_help();
            Ok(())
        }
        _ => {
            eprintln!("unknown command: {cmd}");
            print_help();
            std::process::exit(2);
        }
    }
}

fn print_help() {
    eprintln!("Veil -- keep secrets on your machine");
    eprintln!();
    eprintln!("  Double-click veil.exe, or run:  veil");
    eprintln!("  Then run:  veil setup");
    eprintln!();
    eprintln!("  veil start              start in background");
    eprintln!("  veil start --foreground");
    eprintln!("  veil setup [--clients LIST] [--upstream URL]");
    eprintln!("      clients: claude,codex,pi,codebuddy,grok,hermes,trae,all");
    eprintln!("  veil update [--check]   download latest GitHub release");
    eprintln!("  veil ui                 open the console");
    eprintln!("  veil status");
    eprintln!("  veil stop");
}

fn ui_url(cfg: &Config) -> String {
    format!("http://{}:{}/", cfg.bind, cfg.management_port)
}

/// Build the UI URL with the admin token embedded as a query param so the
/// browser page can auto-populate the token field on first open.
/// Falls back to the plain URL if the token file cannot be read.
fn ui_url_with_token(cfg: &Config, data_dir: &std::path::Path) -> String {
    let base = ui_url(cfg);
    let token_path = data_dir.join("admin.token");
    if let Ok(raw) = std::fs::read_to_string(&token_path) {
        let token = raw.trim();
        if !token.is_empty() {
            return format!("{base}?token={token}");
        }
    }
    base
}

fn open_browser(url: &str) {
    let _ = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
}

/// 打印服务就绪信息。
/// 控制台地址必须带上 `?token=`，否则用户在新标签页/重开浏览器后进入
/// 控制台，保存上游、重启转发等写操作会因缺少管理令牌返回 401。
fn print_ready(cfg: &Config, data_dir: &std::path::Path) {
    let ui = ui_url_with_token(cfg, data_dir);
    println!("Veil is running");
    println!("  console: {ui}");
    println!("  proxy:   {DEFAULT_PROXY_URL}  (default)");
    for r in &cfg.routes {
        println!(
            "  {:<12} http://127.0.0.1:{}  →  {}",
            r.id, r.port, r.upstream
        );
    }
    println!();
    println!("Next:  veil setup   (reads each app's current API URL)");
}

async fn cmd_start(foreground: bool) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = default_data_dir();
    let cfg = Config::load(&data_dir)?;
    if !foreground {
        let _startup_lock = loop {
            match process::acquire_startup_lock(&data_dir) {
                Ok(lock) => break lock,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if probe_lifecycle(cfg.management_port).await.is_some() {
                        print_ready(&cfg, &data_dir);
                        open_browser(&ui_url_with_token(&cfg, &data_dir));
                        return Ok(());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Err(e) => return Err(e.into()),
            }
        };
        if probe_lifecycle(cfg.management_port).await.is_some() {
            print_ready(&cfg, &data_dir);
            open_browser(&ui_url_with_token(&cfg, &data_dir));
            return Ok(());
        }
        process::spawn_daemon(&process::current_exe())?;
        let mut ready = false;
        for _ in 0..50 {
            if probe_lifecycle(cfg.management_port).await.is_some() {
                ready = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if !ready {
            let detail = process::read_runtime(&data_dir)?
                .and_then(|m| m.last_lifecycle_error)
                .unwrap_or_else(|| {
                    "the background process did not expose its control plane within 5 seconds"
                        .into()
                });
            return Err(detail.into());
        }
        print_ready(&cfg, &data_dir);
        open_browser(&ui_url_with_token(&cfg, &data_dir));
        return Ok(());
    }
    match process::acquire_instance_lock(&data_dir) {
        Ok(instance_lock) => {
            if let Some(meta) = process::read_runtime(&data_dir)? {
                if process::metadata_matches_process(&meta)
                    && probe_lifecycle(meta.management_port).await.is_none()
                {
                    // Management API is unresponsive (condition 5 satisfied via
                    // the "no response" branch); attempt safe recovery.
                    if !process::terminate_confirmed(&meta, None)? {
                        return Err(format!(
                            "runtime metadata points to live PID {}, but its identity could not be safely confirmed for recovery",
                            meta.pid
                        )
                        .into());
                    }
                    for _ in 0..30 {
                        if !process::pid_is_alive(meta.pid) {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            }
            let _ = process::remove_stale_runtime(&data_dir);
            serve(&data_dir, cfg, instance_lock).await
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if probe_lifecycle(cfg.management_port).await.is_some() {
                print_ready(&cfg, &data_dir);
                Ok(())
            } else {
                Err("another Veil instance owns this data directory but its control plane is not responding".into())
            }
        }
        Err(e) => Err(e.into()),
    }
}

async fn serve(
    data_dir: &std::path::Path,
    cfg: Config,
    _instance_lock: process::InstanceLock,
) -> Result<(), Box<dyn std::error::Error>> {
    logging::init(data_dir);
    let key = load_or_create(data_dir, cfg.mode)?;
    let store = Arc::new(
        SqliteStore::open(data_dir.join("mappings.db"), &key)
            .map_err(|e| format!("mapping store: {e}"))?,
    );
    let traffic = TrafficLog::default();
    let loopback = cfg.bind == "127.0.0.1" || cfg.bind == "localhost" || cfg.bind == "::1";
    let management_port = cfg.management_port;
    let mgmt_bind = format!("{}:{}", cfg.bind, management_port);
    let listener = TcpListener::bind(&mgmt_bind).await.map_err(|e| {
        format!(
            "cannot bind management port {management_port}: {e}; {}",
            process::describe_port_owner(management_port)
        )
    })?;
    let instance_id = ulid::Ulid::new().to_string();
    let started_at = process::now_rfc3339();
    let config = Arc::new(Mutex::new(cfg.clone()));
    let (exit_tx, mut exit_rx) = tokio::sync::watch::channel(false);
    let lifecycle = lifecycle::spawn(LifecycleParams {
        data_dir: data_dir.to_path_buf(),
        config: config.clone(),
        store,
        traffic: traffic.clone(),
        instance_id: instance_id.clone(),
        started_at: started_at.clone(),
        management_port,
        exit_tx,
    });
    process::write_runtime(
        data_dir,
        &RuntimeMetadata {
            pid: std::process::id(),
            process_started_at: process::current_process_started_at(),
            instance_id: instance_id.clone(),
            exe_path: process::current_exe(),
            management_port,
            listener_ports: vec![management_port],
            proxy_state: "starting".into(),
            started_at,
            last_lifecycle_error: None,
        },
    )?;
    let admin = AdminState {
        data_dir: data_dir.to_path_buf(),
        config,
        loopback,
        traffic,
        proxy_port: cfg.proxy_port,
        management_port,
        lifecycle: Some(lifecycle.clone()),
    };
    let mgmt = admin::router(admin);
    if let Err(error) = lifecycle.start().await {
        eprintln!("data plane did not start: {error}");
    }
    print_ready(&cfg, &data_dir);
    let result = axum::serve(listener, mgmt)
        .with_graceful_shutdown(async move {
            while !*exit_rx.borrow() {
                if exit_rx.changed().await.is_err() {
                    break;
                }
            }
        })
        .await;
    let cleanup = process::remove_runtime_if_instance(data_dir, &instance_id);
    result?;
    cleanup?;
    Ok(())
}

async fn cmd_stop() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = default_data_dir();
    let cfg = Config::load(&data_dir)?;
    let token = std::fs::read_to_string(data_dir.join("admin.token")).unwrap_or_default();
    let url = format!(
        "http://127.0.0.1:{}/api/lifecycle/exit",
        cfg.management_port
    );
    let result = direct_client()
        .post(url)
        .header("x-veil-admin-token", token.trim())
        .send()
        .await;
    if result.as_ref().is_ok_and(|r| r.status().is_success()) {
        for _ in 0..50 {
            if probe_lifecycle(cfg.management_port).await.is_none() {
                println!("stopped");
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    if let Some(meta) = process::read_runtime(&data_dir)? {
        // Probe the management API so terminate_confirmed can apply the
        // instance_id check (design §4.3 condition 5): if the API responds,
        // it must report the same instance_id; otherwise this PID belongs to
        // a different Veil and must not be auto-killed.
        let probed = probe_lifecycle(meta.management_port).await;
        let api_id = probed.as_ref().map(|p| p.instance_id.as_str());
        if process::terminate_confirmed(&meta, api_id)? {
            process::remove_runtime_if_instance(&data_dir, &meta.instance_id)?;
            println!("stopped (forced after control plane did not respond)");
            return Ok(());
        }
        return Err(format!(
            "Veil did not stop gracefully and PID {} could not be safely confirmed as this instance",
            meta.pid
        )
        .into());
    }
    println!("not running");
    Ok(())
}

async fn cmd_status() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = default_data_dir();
    let cfg = Config::load(&data_dir)?;
    if let Some(status) = probe_lifecycle(cfg.management_port).await {
        println!(
            "control_plane={} proxy_state={}",
            status.control_plane, status.proxy_state
        );
        println!(
            "pid={} instance_id={} started_at={}",
            status.pid, status.instance_id, status.started_at
        );
        println!(
            "console=http://127.0.0.1:{} listeners={:?}",
            cfg.management_port, status.listeners
        );
        if let Some(error) = status.last_lifecycle_error {
            println!("last_lifecycle_error={error}");
        }
        return Ok(());
    }
    match process::read_runtime(&data_dir)? {
        Some(meta) if process::metadata_matches_process(&meta) => println!(
            "unresponsive pid={} instance_id={} console={}",
            meta.pid, meta.instance_id, meta.management_port
        ),
        Some(meta) => println!("stale runtime metadata pid={} (not running)", meta.pid),
        None => println!("not running. start with: veil"),
    }
    Ok(())
}

#[derive(serde::Deserialize)]
struct CliLifecycle {
    instance_id: String,
    pid: u32,
    started_at: String,
    control_plane: String,
    proxy_state: String,
    listeners: Vec<u16>,
    last_lifecycle_error: Option<String>,
}

fn direct_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .expect("local control client")
}

async fn probe_lifecycle(port: u16) -> Option<CliLifecycle> {
    direct_client()
        .get(format!("http://127.0.0.1:{port}/api/lifecycle"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()
}

fn cmd_ui(open: bool) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = default_data_dir();
    let cfg = Config::load(&data_dir)?;
    // 控制台地址必须带上管理令牌，否则页面内的写操作（保存上游 / 重启转发）会 401。
    let url = ui_url_with_token(&cfg, &data_dir);
    println!("{url}");
    if open {
        open_browser(&url);
    }
    Ok(())
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find_map(|w| {
        if w[0] == name {
            Some(w[1].clone())
        } else {
            None
        }
    })
}

fn pick_clients(args: &[String]) -> Result<Vec<&'static str>, Box<dyn std::error::Error>> {
    if let Some(raw) = flag_value(args, "--clients") {
        return Ok(clients::resolve_ids(&raw)?);
    }
    let detected = clients::detected_ids();
    if args.iter().any(|a| a == "--yes" || a == "-y") || !io::stdin().is_terminal() {
        if detected.is_empty() {
            return Ok(vec!["claude"]);
        }
        return Ok(detected);
    }
    println!("Which apps should send traffic through Veil?");
    for (i, spec) in clients::CLIENTS.iter().enumerate() {
        let mark = if clients::is_detected(spec.id) {
            "found"
        } else {
            "     "
        };
        println!("  {}) [{:5}] {}", i + 1, mark, spec.label);
    }
    print!("Numbers, 'all', or Enter for all found: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let line = line.trim();
    if line.is_empty() {
        if detected.is_empty() {
            return Ok(clients::CLIENTS.iter().map(|c| c.id).collect());
        }
        return Ok(detected);
    }
    if line.eq_ignore_ascii_case("all") {
        return Ok(clients::CLIENTS.iter().map(|c| c.id).collect());
    }
    let mut ids = Vec::new();
    for part in line.split(|c: char| c == ',' || c.is_whitespace()) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Ok(n) = part.parse::<usize>() {
            if let Some(spec) = clients::CLIENTS.get(n.saturating_sub(1)) {
                if !ids.contains(&spec.id) {
                    ids.push(spec.id);
                }
                continue;
            }
        }
        ids.extend(clients::resolve_ids(part)?);
    }
    Ok(ids)
}

fn cmd_setup(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let ids = pick_clients(args)?;
    if ids.is_empty() {
        println!("no clients selected");
        return Ok(());
    }
    let explicit = flag_value(args, "--upstream");
    let data_dir = default_data_dir();
    let mut cfg = Config::load(&data_dir)?;
    if let Some(url) = explicit.as_deref() {
        setup::apply_chained_upstream(&mut cfg, Some(url));
    }
    println!("each app keeps its own upstream behind a local port");
    for id in &ids {
        let (r, route) = clients::wire_client(&mut cfg, id, explicit.as_deref())?;
        println!(
            "  {}  http://127.0.0.1:{}  →  {}  ({})",
            r.id,
            route.port,
            route.upstream,
            r.path.display()
        );
    }
    cfg.save(&data_dir)?;
    println!();
    println!(
        "Restart the selected apps. Restart Veil if it was already running, so new ports listen."
    );
    println!("API keys stay in the client; Veil pass-throughs them.");
    Ok(())
}

async fn cmd_update(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let check_only = args.iter().any(|a| a == "--check");
    let info = update::check().await?;
    println!("current {}  latest {}", info.current, info.latest);
    if !info.newer {
        println!("already up to date");
        return Ok(());
    }
    if check_only {
        println!("update available: {}", info.asset);
        return Ok(());
    }
    let exe = update::current_exe()?;
    let staged = update::staged_next_to(&exe);
    println!("downloading {}", info.asset);
    update::download_and_stage(&staged).await?;
    update::schedule_replace(&exe, &staged)?;
    println!("saved {}. a helper will swap the exe and restart in a few seconds — you can close this window.", info.latest);
    Ok(())
}
