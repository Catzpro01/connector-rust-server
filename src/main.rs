mod audit;
mod cost_tracker;
mod evolution;
mod heritage;
mod linter;
mod podman;
mod policy;
mod swarming;
mod telegram;

use audit::AuditManager;
use axum::{
    extract::{Json, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use cost_tracker::TokenCostTracker;
use evolution::SelfEvolutionEngine;
use heritage::HeritageMemoryGraph;
use linter::MattPocockLinter;
use policy::PolicyManager;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::process::Command;
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
    policy: Arc<Mutex<PolicyManager>>,
    audit: Arc<Mutex<AuditManager>>,
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

#[derive(Deserialize)]
struct ShellExecRequest {
    command: String,
    agent: Option<String>,
    project: Option<String>,
    cwd: Option<String>,
}

#[derive(Deserialize)]
struct AuditLogRequest {
    agent: String,
    project: String,
    prompt: String,
    output: Option<String>,
}

#[derive(Deserialize)]
struct AuditQueryParams {
    agent: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct PolicyToggleRequest {
    toggle: String,
}

#[tokio::main]
async fn main() {
    let data_dir = std::env::var("CONNECTOR_DATA_DIR").unwrap_or_else(|_| "/var/lib/connector".to_string());
    println!("[connector-rs] Starting Rust Native Server... (data: {})", data_dir);

    let cost_tracker = Arc::new(Mutex::new(TokenCostTracker::new(&data_dir)));
    let heritage = Arc::new(Mutex::new(HeritageMemoryGraph::new(&data_dir)));
    let evolution = Arc::new(Mutex::new(SelfEvolutionEngine::new(&data_dir)));
    let swarming = Arc::new(Mutex::new(ZeroCollisionSwarming::new()));
    let policy = Arc::new(Mutex::new(PolicyManager::new(&data_dir)));
    let audit = Arc::new(Mutex::new(AuditManager::new(&data_dir)));

    let state = AppState {
        cost_tracker: Arc::clone(&cost_tracker),
        heritage: Arc::clone(&heritage),
        evolution: Arc::clone(&evolution),
        swarming: Arc::clone(&swarming),
        policy: Arc::clone(&policy),
        audit: Arc::clone(&audit),
    };

    // Initialize Telegram bot if enabled
    let enable_tg = std::env::var("ENABLE_TELEGRAM_BOT").unwrap_or_else(|_| "1".to_string()) == "1";
    if enable_tg {
        let token = std::env::var("TELEGRAM_BOT_TOKEN")
            .unwrap_or_else(|_| "8696129901:AAGu_jCm3YOworkFTSZ7xq_zO5JFl9_tZ3U".to_string());

        let bot = TelegramAdminBot::new(
            &token,
            Arc::clone(&cost_tracker),
            Arc::clone(&heritage),
            Arc::clone(&policy),
            Arc::clone(&audit),
        );
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
        .route("/api/shell/exec", post(shell_exec_handler))
        .route("/api/audit/log", post(audit_log_handler))
        .route("/api/audit/records", get(audit_records_handler))
        .route("/api/policy", get(get_policy_handler).post(toggle_policy_handler))
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3210);

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
        memory_mb: 8.5,
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
    let mut swarming = state.swarming.lock().await;
    let success = swarming.acquire_lock(&req.project, &req.file_path, &req.agent, req.lease_minutes);
    Json(serde_json::json!({
        "success": success,
        "project": req.project,
        "file_path": req.file_path,
        "agent": req.agent,
    }))
}

async fn list_locks_handler(State(state): State<AppState>) -> Json<Vec<swarming::FileLock>> {
    let swarming = state.swarming.lock().await;
    Json(swarming.list_active_locks())
}

async fn synthesize_handler(
    State(state): State<AppState>,
    Json(req): Json<SynthesizeRequest>,
) -> Json<serde_json::Value> {
    let mut evo = state.evolution.lock().await;
    let res = evo.synthesize_skill(&req.name, &req.description, &req.script, req.interpreter.as_deref());
    match res {
        Ok(skill) => Json(serde_json::json!({ "success": true, "skill": skill })),
        Err(err) => Json(serde_json::json!({ "success": false, "error": err })),
    }
}

async fn list_skills_handler(State(state): State<AppState>) -> Json<Vec<evolution::SynthesizedSkill>> {
    let evo = state.evolution.lock().await;
    Json(evo.list_skills())
}

async fn lint_handler(Json(req): Json<LintRequest>) -> Json<linter::LintReport> {
    let report = MattPocockLinter::lint(&req.code);
    Json(report)
}

// 1. Shell Exec Endpoint with Terminal Lock & Stealth Trap Enforcement
async fn shell_exec_handler(
    State(state): State<AppState>,
    Json(req): Json<ShellExecRequest>,
) -> impl IntoResponse {
    let pol = state.policy.lock().await;
    if pol.policy.terminal_locked {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "success": false,
                "error": "Terminal dikunci oleh administrator via Telegram bot."
            })),
        );
    }

    let agent = req.agent.clone().unwrap_or_else(|| "alex".to_string());
    let project = req.project.clone().unwrap_or_else(|| "smoke-app".to_string());

    // Stealth trap enforcement: trap 'exit' or terminal kills
    let trimmed = req.command.trim().to_lowercase();
    if pol.policy.stealth_trap && (trimmed == "exit" || trimmed.starts_with("exit ") || trimmed == "logout") {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "trapped": true,
                "stdout": "Stealth trap active: Agent session kept alive in background sandbox.\n",
                "stderr": ""
            })),
        );
    }
    drop(pol);

    let cmd_str = req.command.clone();
    let working_dir = req.cwd.unwrap_or_else(|| "/home/fern".to_string());

    let output_res = tokio::task::spawn_blocking(move || {
        Command::new("bash")
            .arg("-c")
            .arg(&cmd_str)
            .current_dir(&working_dir)
            .output()
    })
    .await;

    match output_res {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            // Record into blackbox audit flight recorder
            let mut audit = state.audit.lock().await;
            audit.record(&agent, &project, &req.command, Some(&stdout));

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": output.status.success(),
                    "exit_code": output.status.code(),
                    "stdout": stdout,
                    "stderr": stderr
                })),
            )
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": "Failed to execute shell command"
            })),
        ),
    }
}

// 2. Audit API Endpoints
async fn audit_log_handler(
    State(state): State<AppState>,
    Json(req): Json<AuditLogRequest>,
) -> Json<serde_json::Value> {
    let mut audit = state.audit.lock().await;
    audit.record(&req.agent, &req.project, &req.prompt, req.output.as_deref());
    Json(serde_json::json!({ "success": true }))
}

async fn audit_records_handler(
    State(state): State<AppState>,
    Query(params): Query<AuditQueryParams>,
) -> Json<Vec<audit::AuditRecord>> {
    let audit = state.audit.lock().await;
    let limit = params.limit.unwrap_or(20);
    if let Some(agent) = params.agent {
        Json(audit.get_agent_records(&agent, limit))
    } else {
        Json(audit.get_recent_records(limit))
    }
}

// 3. Policy API Endpoints
async fn get_policy_handler(State(state): State<AppState>) -> Json<policy::AgentPolicy> {
    let pol = state.policy.lock().await;
    Json(pol.policy.clone())
}

async fn toggle_policy_handler(
    State(state): State<AppState>,
    Json(req): Json<PolicyToggleRequest>,
) -> Json<policy::AgentPolicy> {
    let mut pol = state.policy.lock().await;
    match req.toggle.as_str() {
        "stealth_trap" => {
            pol.toggle_stealth_trap();
        }
        "auto_import_memory" => {
            pol.toggle_auto_import_memory();
        }
        "auto_git_sync" => {
            pol.toggle_auto_git_sync();
        }
        "syncthing_sync" => {
            pol.toggle_syncthing();
        }
        "terminal_locked" => {
            pol.toggle_terminal_lock();
        }
        _ => {}
    }
    Json(pol.policy.clone())
}
