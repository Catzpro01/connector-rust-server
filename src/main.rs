mod cost_tracker;
mod evolution;
mod heritage;
mod linter;
mod podman;
mod swarming;
mod telegram;

use axum::{
    extract::{Json, State},
    routing::{get, post},
    Router,
};
use cost_tracker::TokenCostTracker;
use evolution::SelfEvolutionEngine;
use heritage::HeritageMemoryGraph;
use linter::MattPocockLinter;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use swarming::ZeroCollisionSwarming;
use telegram::TelegramAdminBot;
use tokio::sync::Mutex;

#[derive(Clone)]
struct AppState {
    cost_tracker: Arc<Mutex<TokenCostTracker>>,
    heritage: Arc<Mutex<HeritageMemoryGraph>>,
    evolution: Arc<Mutex<SelfEvolutionEngine>>,
    swarming: Arc<Mutex<ZeroCollisionSwarming>>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    engine: &'static str,
    version: &'static str,
    active_skills: usize,
    memory_mb: f64,
}

#[derive(Deserialize)]
struct SynthesizeRequest {
    name: String,
    description: String,
    script: String,
    interpreter: Option<String>,
}

#[derive(Deserialize)]
struct LockRequest {
    project: String,
    file_path: String,
    agent: String,
    lease_minutes: Option<u64>,
}

#[derive(Deserialize)]
struct LintRequest {
    code: String,
}

#[tokio::main]
async fn main() {
    let data_dir = std::env::var("CONNECTOR_DATA_DIR").unwrap_or_else(|_| "/var/lib/connector".to_string());
    println!("[connector-rs] Starting Rust Native Server... (data: {})", data_dir);

    let cost_tracker = Arc::new(Mutex::new(TokenCostTracker::new(&data_dir)));
    let heritage = Arc::new(Mutex::new(HeritageMemoryGraph::new(&data_dir)));
    let evolution = Arc::new(Mutex::new(SelfEvolutionEngine::new(&data_dir)));
    let swarming = Arc::new(Mutex::new(ZeroCollisionSwarming::new()));

    let state = AppState {
        cost_tracker: Arc::clone(&cost_tracker),
        heritage: Arc::clone(&heritage),
        evolution: Arc::clone(&evolution),
        swarming: Arc::clone(&swarming),
    };

    // Initialize Telegram bot if enabled
    let enable_tg = std::env::var("ENABLE_TELEGRAM_BOT").unwrap_or_else(|_| "1".to_string()) == "1";
    if enable_tg {
        let token = std::env::var("TELEGRAM_BOT_TOKEN")
            .unwrap_or_else(|_| "8696129901:AAGu_jCm3YOworkFTSZ7xq_zO5JFl9_tZ3U".to_string());

        let bot = TelegramAdminBot::new(&token, Arc::clone(&cost_tracker), Arc::clone(&heritage));
        bot.set_admin_chat_id(5602465864);

        tokio::spawn(async move {
            bot.run_poll_loop().await;
        });
    } else {
        println!("[connector-rs] Telegram bot disabled by ENABLE_TELEGRAM_BOT=0");
    }

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/api/cost", get(cost_handler))
        .route("/api/swarming/lock", post(acquire_lock_handler))
        .route("/api/swarming/locks", get(list_locks_handler))
        .route("/api/evolution/synthesize", post(synthesize_handler))
        .route("/api/evolution/skills", get(list_skills_handler))
        .route("/api/lint/mattpocock", post(lint_handler))
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3211);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("[connector-rs] Listening on http://{} (Native Rust Standalone)", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.expect("Failed to bind port");
    axum::serve(listener, app).await.expect("Server error");
}

async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let evo = state.evolution.lock().await;
    Json(HealthResponse {
        status: "ok",
        engine: "rust-native-standalone",
        version: "0.1.0",
        active_skills: evo.list_skills().len(),
        memory_mb: 8.5, // Native footprint
    })
}

async fn cost_handler(State(state): State<AppState>) -> Json<cost_tracker::CostSummary> {
    let tracker = state.cost_tracker.lock().await;
    Json(tracker.get_global_summary())
}

async fn acquire_lock_handler(
    State(state): State<AppState>,
    Json(req): Json<LockRequest>,
) -> Json<serde_json::Value> {
    let mut sw = state.swarming.lock().await;
    match sw.acquire_lock(&req.project, &req.file_path, &req.agent, req.lease_minutes.unwrap_or(10)) {
        Ok(msg) => Json(serde_json::json!({ "success": true, "message": msg })),
        Err(err) => Json(serde_json::json!({ "success": false, "error": err })),
    }
}

async fn list_locks_handler(State(state): State<AppState>) -> Json<Vec<swarming::FileLock>> {
    let sw = state.swarming.lock().await;
    Json(sw.list_active_locks(None))
}

async fn synthesize_handler(
    State(state): State<AppState>,
    Json(req): Json<SynthesizeRequest>,
) -> Json<serde_json::Value> {
    let mut evo = state.evolution.lock().await;
    let interpreter = req.interpreter.as_deref().unwrap_or("sh");
    match evo.synthesize_script_tool(&req.name, &req.description, &req.script, interpreter) {
        Ok(skill) => Json(serde_json::json!({ "success": true, "skill": skill })),
        Err(err) => Json(serde_json::json!({ "success": false, "error": err })),
    }
}

async fn list_skills_handler(State(state): State<AppState>) -> Json<Vec<evolution::SynthesizedSkill>> {
    let evo = state.evolution.lock().await;
    Json(evo.list_skills().to_vec())
}

async fn lint_handler(Json(req): Json<LintRequest>) -> Json<serde_json::Value> {
    let violations = MattPocockLinter::scan_ts_code(&req.code);
    let compliant = violations.is_empty();
    Json(serde_json::json!({
        "compliant": compliant,
        "violation_count": violations.len(),
        "violations": violations
    }))
}
