//! Web interface and JSON API, for servers without a display.
//!
//! - `GET /`: the page (embedded, no external assets, works offline).
//! - `GET /api/disks`, `GET /api/disks/{name}`: the reports, as `disklens list --json`.
//! - `GET /api/history`: recent temperature / life samples per disk.
//! - `POST /api/refresh`: scan now.
//! - `GET /metrics`: Prometheus text format, for Grafana and alerting.
//! - `GET /healthz`: liveness.
//!
//! It listens on 127.0.0.1 by default. With `--token` (or `DISKLENS_TOKEN`) every request
//! needs `Authorization: Bearer <token>` or `?token=<token>`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, RwLock};

use disklens::scan::{self, History, ScanOptions};
use disklens::smart::{DiskReport, Health};

const PAGE: &str = include_str!("../assets/index.html");

#[derive(Default, Serialize)]
struct Snapshot {
    updated: Option<String>,
    privileged: bool,
    interval_s: u64,
    disks: Vec<DiskReport>,
}

struct AppState {
    snap: RwLock<Snapshot>,
    history: RwLock<History>,
    wake: Notify,
    token: Option<String>,
}

type Shared = Arc<AppState>;

pub fn run(
    opts: ScanOptions,
    bind: &str,
    interval: u64,
    token: Option<String>,
    history: usize,
) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(serve(
        opts,
        bind.to_string(),
        interval.max(5),
        token,
        history,
    ))
}

async fn serve(
    opts: ScanOptions,
    bind: String,
    interval: u64,
    token: Option<String>,
    history: usize,
) -> Result<()> {
    let state = Arc::new(AppState {
        snap: RwLock::new(Snapshot {
            interval_s: interval,
            privileged: disklens::device::privileged() || !opts.from.is_empty(),
            ..Default::default()
        }),
        history: RwLock::new(History::new(history)),
        wake: Notify::new(),
        token: token.filter(|t| !t.is_empty()),
    });

    // Scanner: every `interval` seconds, or right away when /api/refresh wakes it.
    let s = state.clone();
    tokio::spawn(async move {
        loop {
            let o = opts.clone();
            if let Ok(reports) = tokio::task::spawn_blocking(move || scan::scan(&o)).await {
                s.history.write().await.record(&reports);
                let mut snap = s.snap.write().await;
                snap.disks = reports;
                snap.updated = Some(chrono::Local::now().to_rfc3339());
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(interval)) => {}
                _ = s.wake.notified() => {}
            }
        }
    });

    let app = Router::new()
        .route("/", get(|| async { Html(PAGE) }))
        .route("/api/disks", get(disks))
        .route("/api/disks/{name}", get(disk))
        .route("/api/history", get(history_all))
        .route("/api/refresh", post(refresh))
        .route("/metrics", get(metrics))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!("disklens web on http://{}", listener.local_addr()?);
    if state.token.is_none()
        && !bind.starts_with("127.")
        && !bind.starts_with("[::1]")
        && !bind.starts_with("localhost")
    {
        eprintln!("warning: listening beyond localhost without --token; anyone on the network can read the disk data");
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

/// Constant-time comparison, so the token cannot be guessed byte by byte from timings.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

async fn auth(
    State(s): State<Shared>,
    Query(q): Query<TokenQuery>,
    req: Request,
    next: Next,
) -> Response {
    let Some(expected) = &s.token else {
        return next.run(req).await;
    };
    let bearer = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let given = bearer.map(str::to_string).or(q.token);
    // The page itself carries no data; let it load so it can ask for the token.
    if req.uri().path() == "/" || given.as_deref().is_some_and(|g| same(g, expected)) {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, "token required").into_response()
    }
}

async fn disks(State(s): State<Shared>) -> impl IntoResponse {
    Json(serde_json::to_value(&*s.snap.read().await).unwrap_or_default())
}

async fn disk(State(s): State<Shared>, Path(name): Path<String>) -> Response {
    let snap = s.snap.read().await;
    match snap.disks.iter().find(|d| d.name == name) {
        Some(d) => Json(d.clone()).into_response(),
        None => (StatusCode::NOT_FOUND, "no such disk").into_response(),
    }
}

async fn history_all(State(s): State<Shared>) -> impl IntoResponse {
    let snap = s.snap.read().await;
    let h = s.history.read().await;
    let out: serde_json::Map<String, serde_json::Value> = snap
        .disks
        .iter()
        .map(|d| {
            (
                d.name.clone(),
                serde_json::to_value(h.of(d)).unwrap_or_default(),
            )
        })
        .collect();
    Json(out)
}

async fn refresh(State(s): State<Shared>) -> impl IntoResponse {
    s.wake.notify_one();
    StatusCode::ACCEPTED
}

fn label(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}

/// Prometheus exposition format (text 0.0.4).
pub fn prometheus(disks: &[DiskReport]) -> String {
    let mut out = String::new();
    let metric = |out: &mut String, name: &str, help: &str| {
        out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} gauge\n"));
    };
    let lbl = |d: &DiskReport| {
        format!(
            "disk=\"{}\",model=\"{}\",serial=\"{}\"",
            label(&d.name),
            label(&d.model),
            label(&d.serial)
        )
    };
    metric(
        &mut out,
        "disklens_health",
        "0 good, 1 caution, 2 bad, 3 unknown (CrystalDiskInfo rules)",
    );
    for d in disks {
        let v = match d.health {
            Health::Good => 0,
            Health::Caution => 1,
            Health::Bad => 2,
            Health::Unknown => 3,
        };
        out.push_str(&format!("disklens_health{{{}}} {v}\n", lbl(d)));
    }
    type Series = (&'static str, &'static str, fn(&DiskReport) -> Option<f64>);
    let series: [Series; 6] = [
        ("disklens_temperature_celsius", "Current temperature", |d| {
            d.temperature_c.map(f64::from)
        }),
        ("disklens_life_percent", "Remaining SSD life", |d| {
            d.life_percent.map(f64::from)
        }),
        ("disklens_power_on_hours", "Power-on hours", |d| {
            d.power_on_hours.map(|v| v as f64)
        }),
        ("disklens_power_cycles", "Power-on count", |d| {
            d.power_on_count.map(|v| v as f64)
        }),
        (
            "disklens_host_writes_gigabytes",
            "Host writes (GB as CrystalDiskInfo counts them)",
            |d| d.host_writes_gb.map(|v| v as f64),
        ),
        ("disklens_host_reads_gigabytes", "Host reads", |d| {
            d.host_reads_gb.map(|v| v as f64)
        }),
    ];
    for (name, help, f) in series {
        metric(&mut out, name, help);
        for d in disks {
            if let Some(v) = f(d) {
                out.push_str(&format!("{name}{{{}}} {v}\n", lbl(d)));
            }
        }
    }
    out
}

async fn metrics(State(s): State<Shared>) -> impl IntoResponse {
    let body = prometheus(&s.snap.read().await.disks);
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body)
}
