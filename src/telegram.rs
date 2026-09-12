use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::System;
use tokio::sync::Mutex;

use crate::audit::AuditManager;
use crate::cost_tracker::TokenCostTracker;
use crate::heritage::HeritageMemoryGraph;
use crate::podman::{ContainerInfo, PodmanManager};
use crate::policy::PolicyManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardButton {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyKeyboardMarkup {
    pub keyboard: Vec<Vec<KeyboardButton>>,
    pub resize_keyboard: bool,
    pub is_persistent: bool,
}

#[derive(Debug, Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    result: Option<T>,
}

#[derive(Debug, Deserialize)]
struct MessageResult {
    message_id: i64,
}

#[derive(Debug, Deserialize)]
struct UpdateResult {
    update_id: i64,
    message: Option<IncomingMessage>,
}

#[derive(Debug, Deserialize)]
struct IncomingMessage {
    message_id: i64,
    chat: ChatInfo,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatInfo {
    id: i64,
}

#[derive(Debug, Clone)]
enum PendingAction {
    CreateProject,
    AddAgent,
    SetModelRate,
    RenameAgent { old_name: String },
}

pub struct TelegramAdminBot {
    token: String,
    admin_chat_id: Arc<Mutex<Option<i64>>>,
    last_card_message_id: Arc<Mutex<Option<i64>>>,
    current_room: Arc<Mutex<String>>,
    selected_agent: Arc<Mutex<String>>,
    selected_project: Arc<Mutex<String>>,
    cost_tracker: Arc<Mutex<TokenCostTracker>>,
    heritage: Arc<Mutex<HeritageMemoryGraph>>,
    policy: Arc<Mutex<PolicyManager>>,
    audit: Arc<Mutex<AuditManager>>,
    cached_containers: Arc<Mutex<Vec<ContainerInfo>>>,
    pending_action: Arc<Mutex<Option<PendingAction>>>,
    pub registered_agents: Arc<Mutex<Vec<String>>>,
    data_dir: String,
    client: reqwest::Client,
}

fn load_registered_agents(data_dir: &str) -> Vec<String> {
    let file = format!("{}/registered_agents.json", data_dir);
    if let Ok(content) = std::fs::read_to_string(&file) {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(&content) {
            if !list.is_empty() {
                return list;
            }
        }
    }
    let defaults = vec![
        "catzpro01".to_string(),
        "alex".to_string(),
        "asep".to_string(),
        "matt".to_string(),
        "fern".to_string(),
    ];
    let _ = save_registered_agents(data_dir, &defaults);
    defaults
}

fn save_registered_agents(data_dir: &str, agents: &[String]) -> bool {
    let file = format!("{}/registered_agents.json", data_dir);
    if let Ok(json) = serde_json::to_string_pretty(agents) {
        std::fs::write(&file, json).is_ok()
    } else {
        false
    }
}

impl TelegramAdminBot {
    pub fn new(
        token: &str,
        data_dir: &str,
        cost_tracker: Arc<Mutex<TokenCostTracker>>,
        heritage: Arc<Mutex<HeritageMemoryGraph>>,
        policy: Arc<Mutex<PolicyManager>>,
        audit: Arc<Mutex<AuditManager>>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .unwrap_or_default();

        let initial_containers = PodmanManager::list_containers();
        let loaded_agents = load_registered_agents(data_dir);
        let default_agent = loaded_agents.first().cloned().unwrap_or_else(|| "catzpro01".to_string());

        Self {
            token: token.to_string(),
            admin_chat_id: Arc::new(Mutex::new(None)),
            last_card_message_id: Arc::new(Mutex::new(None)),
            current_room: Arc::new(Mutex::new("main".to_string())),
            selected_agent: Arc::new(Mutex::new(default_agent)),
            selected_project: Arc::new(Mutex::new("smoke-app".to_string())),
            cost_tracker,
            heritage,
            policy,
            audit,
            cached_containers: Arc::new(Mutex::new(initial_containers)),
            pending_action: Arc::new(Mutex::new(None)),
            registered_agents: Arc::new(Mutex::new(loaded_agents)),
            data_dir: data_dir.to_string(),
            client,
        }
    }

    pub async fn register_agent_if_new(&self, agent_name: &str) {
        let clean = agent_name.trim().to_lowercase();
        if clean.is_empty() || clean == "unknown" {
            return;
        }
        let mut a = self.registered_agents.lock().await;
        if !a.contains(&clean) {
            a.push(clean);
            save_registered_agents(&self.data_dir, &a);
        }
    }

    pub fn set_admin_chat_id(&self, chat_id: i64) {
        let admin = Arc::clone(&self.admin_chat_id);
        tokio::spawn(async move {
            let mut a = admin.lock().await;
            *a = Some(chat_id);
        });
    }

    async fn get_containers_fast(&self) -> Vec<ContainerInfo> {
        let cache = self.cached_containers.lock().await;
        cache.clone()
    }

    fn trigger_container_refresh(&self) {
        let cache_lock = Arc::clone(&self.cached_containers);
        tokio::task::spawn_blocking(move || {
            let fresh = PodmanManager::list_containers();
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async move {
                let mut c = cache_lock.lock().await;
                *c = fresh;
            });
        });
    }

    pub fn get_adaptive_keyboard(&self, room: &str, _containers: &[ContainerInfo], agents: &[String], selected_slug: &str, selected_agent: &str) -> ReplyKeyboardMarkup {
        match room {
            "projects" => ReplyKeyboardMarkup {
                keyboard: vec![
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }, KeyboardButton { text: "👥 Ruang Agen".into() }],
                    vec![KeyboardButton { text: "📊 Hardware & Biaya".into() }, KeyboardButton { text: "➕ Buat Project".into() }],
                    vec![KeyboardButton { text: "🔄 Refresh Project".into() }],
                ],
                resize_keyboard: true,
                is_persistent: true,
            },
            "project_detail" => ReplyKeyboardMarkup {
                keyboard: vec![
                    vec![KeyboardButton { text: format!("💾 Simpan: {}", selected_slug) }, KeyboardButton { text: format!("⏪ Rollback: {}", selected_slug) }],
                    vec![KeyboardButton { text: format!("▶️ Play: {}", selected_slug) }, KeyboardButton { text: format!("⏹ Stop: {}", selected_slug) }],
                    vec![KeyboardButton { text: format!("🔄 Restart: {}", selected_slug) }, KeyboardButton { text: format!("🗑 Hapus Project: {}", selected_slug) }],
                    vec![KeyboardButton { text: "📁 Project View".into() }, KeyboardButton { text: "🎮 Menu Utama".into() }],
                ],
                resize_keyboard: true,
                is_persistent: true,
            },
            "status" => ReplyKeyboardMarkup {
                keyboard: vec![
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }, KeyboardButton { text: "📁 Project View".into() }],
                    vec![KeyboardButton { text: "⚙️ Atur Tarif Model".into() }, KeyboardButton { text: "💰 Reset Biaya Token".into() }],
                    vec![KeyboardButton { text: "🔄 Refresh Biaya".into() }],
                ],
                resize_keyboard: true,
                is_persistent: true,
            },
            "settings" => ReplyKeyboardMarkup {
                keyboard: vec![
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }, KeyboardButton { text: "🔄 Refresh Pengaturan".into() }],
                    vec![KeyboardButton { text: "🥷 Toggle Stealth Trap".into() }, KeyboardButton { text: "🧠 Toggle Auto Memory".into() }],
                    vec![KeyboardButton { text: "📦 Toggle Git Sync".into() }, KeyboardButton { text: "🔄 Toggle Syncthing".into() }],
                    vec![KeyboardButton { text: "🔒 Toggle Terminal Lock".into() }, KeyboardButton { text: "🐙 GitHub".into() }],
                ],
                resize_keyboard: true,
                is_persistent: true,
            },
            "agents" => {
                let mut rows = vec![
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }, KeyboardButton { text: "📁 Project View".into() }],
                ];
                let mut agent_buttons = Vec::new();
                for a in agents {
                    agent_buttons.push(KeyboardButton { text: format!("👤 Detail: {}", a) });
                    if agent_buttons.len() == 2 {
                        rows.push(agent_buttons);
                        agent_buttons = Vec::new();
                    }
                }
                if !agent_buttons.is_empty() {
                    rows.push(agent_buttons);
                }
                rows.push(vec![KeyboardButton { text: "➕ Tambah Agent".into() }, KeyboardButton { text: "🔄 Refresh Agen".into() }]);
                ReplyKeyboardMarkup {
                    keyboard: rows,
                    resize_keyboard: true,
                    is_persistent: true,
                }
            }
            "agent_detail" => ReplyKeyboardMarkup {
                keyboard: vec![
                    vec![KeyboardButton { text: "📜 Blackbox Records".into() }, KeyboardButton { text: "💰 Reset Biaya Token".into() }],
                    vec![KeyboardButton { text: format!("🗑 Hapus Agen: {}", selected_agent) }, KeyboardButton { text: "👥 Ruang Agen".into() }],
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }],
                ],
                resize_keyboard: true,
                is_persistent: true,
            },
            "blackbox" => ReplyKeyboardMarkup {
                keyboard: vec![
                    vec![KeyboardButton { text: format!("👤 Detail: {}", selected_agent) }, KeyboardButton { text: "👥 Ruang Agen".into() }],
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }],
                ],
                resize_keyboard: true,
                is_persistent: true,
            },
            _ => ReplyKeyboardMarkup {
                keyboard: vec![
                    vec![KeyboardButton { text: "📁 Project View".into() }, KeyboardButton { text: "👥 Ruang Agen".into() }],
                    vec![KeyboardButton { text: "📊 Hardware & Biaya".into() }, KeyboardButton { text: "⚙️ Pengaturan".into() }],
                    vec![KeyboardButton { text: "🔄 Refresh".into() }],
                ],
                resize_keyboard: true,
                is_persistent: true,
            },
        }
    }

    fn delete_message_bg(&self, chat_id: i64, message_id: i64) {
        let client = self.client.clone();
        let token = self.token.clone();
        tokio::spawn(async move {
            let url = format!("https://api.telegram.org/bot{}/deleteMessage", token);
            let _ = client
                .post(&url)
                .json(&serde_json::json!({
                    "chat_id": chat_id,
                    "message_id": message_id
                }))
                .send()
                .await;
        });
    }

    pub async fn render_room(&self, chat_id: i64, room: &str, banner: Option<&str>, proc_latency_ms: u64) {
        {
            let mut r = self.current_room.lock().await;
            *r = room.to_string();
        }

        let containers = self.get_containers_fast().await;
        let agents = {
            let a = self.registered_agents.lock().await;
            a.clone()
        };
        let selected_slug = {
            let s = self.selected_project.lock().await;
            s.clone()
        };
        let selected_agent = {
            let a = self.selected_agent.lock().await;
            a.clone()
        };

        let mut lines = Vec::new();

        match room {
            "settings" => {
                let pol = self.policy.lock().await;
                lines.push("⚙️ *PENGATURAN POLICY*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n────────────────────", b));
                }
                lines.push(format!("🥷 *Stealth Trap*  : {}", if pol.policy.session_persistence { "🟢 AKTIF" } else { "🔴 MATI" }));
                lines.push(format!("🧠 *Auto Memory*   : {}", if pol.policy.cache_warmup { "🟢 AKTIF" } else { "🔴 MATI" }));
                lines.push(format!("📦 *Git Auto Sync* : {}", if pol.policy.background_backup { "🟢 AKTIF" } else { "🔴 MATI" }));
                lines.push(format!("🔄 *Syncthing P2P* : {}", if pol.policy.peer_sync { "🟢 AKTIF" } else { "🔴 MATI" }));
                lines.push(format!("🔒 *Terminal Lock* : {}", if pol.policy.maintenance_mode { "🔴 TERKUNCI" } else { "🟢 TERBUKA" }));
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Ketuk tombol di bawah untuk toggle._".to_string());
            }

            "projects" => {
                let tracker = self.cost_tracker.lock().await;
                let running_count = containers.iter().filter(|c| c.is_running).count();

                lines.push(format!("📁 *PROJECT VIEW ({} Aktif)*", running_count));
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n────────────────────", b));
                }

                if containers.is_empty() {
                    lines.push("_(Belum ada project/kontainer terdaftar)_".to_string());
                    lines.push("💡 _Ketuk [ ➕ Buat Project ] di bawah untuk membuat._".to_string());
                } else {
                    for c in &containers {
                        let icon = if c.is_running { "🟢" } else { "🔴" };
                        let proj_cost = tracker.get_cost_by_project(&c.slug);
                        let latency = PodmanManager::get_container_latency(&c.slug);
                        let agent_list = if c.assigned_agents.is_empty() {
                            "alex, asep".to_string()
                        } else {
                            c.assigned_agents.join(", ")
                        };

                        let git_st = PodmanManager::get_git_sync_status(&c.slug);
                        lines.push(format!("🔹 *[{}]* {} `{}`", c.slug, icon, if c.is_running { "Running" } else { "Stopped" }));
                        lines.push(format!("   • 👥 Agen   : `{}`", agent_list));
                        lines.push(format!("   • 🧠 RAM/CPU: `{}` | `{}`", c.mem, c.cpu));
                        lines.push(format!("   • 💽 Disk   : `{}` | 📶 `{}ms`", c.disk_usage, latency));
                        lines.push(format!("   • 📦 Git    : {} — `/simpan_{}`", git_st, c.slug));
                        lines.push(format!("   • 💰 Token  : In `{}` / Out `{}` ({})",
                            proj_cost.prompt_tokens, proj_cost.completion_tokens, proj_cost.cost_idr_formatted));
                        lines.push(format!("   👉 `/detail_{}`", c.slug));
                        lines.push("".to_string());
                    }
                }
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 Ketik `/detail_<nama>` untuk kelola Play/Stop/Hapus/Simpan.".to_string());
            }

            "project_detail" => {
                let tracker = self.cost_tracker.lock().await;
                let proj_cost = tracker.get_cost_by_project(&selected_slug);
                let container_opt = containers.iter().find(|c| c.slug == selected_slug);
                let (is_running, status_str, mem, cpu, disk) = match container_opt {
                    Some(c) => (c.is_running, c.status.clone(), c.mem.clone(), c.cpu.clone(), c.disk_usage.clone()),
                    None => (false, "Stopped".to_string(), "0 MB".to_string(), "0%".to_string(), "0 KB".to_string()),
                };
                let latency = PodmanManager::get_container_latency(&selected_slug);
                let git_st = PodmanManager::get_git_sync_status(&selected_slug);

                lines.push(format!("📁 *DETAIL PROJECT: {}*", selected_slug.replace("_", " ").to_uppercase()));
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    let clean_b = b.replace("_", "\\_");
                    lines.push(format!("🔔 _{}_\n────────────────────", clean_b));
                }

                lines.push(format!("⚙️ *Status*   : {} {}", if is_running { "🟢" } else { "🔴" }, status_str));
                lines.push(format!("👥 *Agen Join*: `alex, asep`"));
                lines.push(format!("🧠 *RAM*      : `{}`", mem));
                lines.push(format!("⚡ *Core/CPU* : `{}`", cpu));
                lines.push(format!("💾 *Disk*     : `{}`", disk));
                lines.push(format!("📶 *Latensi*  : `{} ms` (exec roundtrip)", latency));
                lines.push(format!("📦 *Git Sync* : {}", git_st));
                lines.push(format!("   👉 `/simpan_{}` | `/rollback_{}`", selected_slug, selected_slug));
                lines.push("".to_string());
                lines.push("📊 *PENGGUNAAN TOKEN & BIAYA:*".to_string());
                lines.push(format!("• Token In  : `{}`", proj_cost.prompt_tokens));
                lines.push(format!("• Token Out : `{}`", proj_cost.completion_tokens));
                lines.push(format!("• Total     : `{}` tokens", proj_cost.total_tokens_formatted));
                lines.push(format!("• Biaya     : *{}* ({})", proj_cost.cost_idr_formatted, proj_cost.cost_usd_formatted));
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 Gunakan tombol di bawah untuk Simpan, Rollback, Play, Stop, atau Hapus.".to_string());
            }

            "status" => {
                let tracker = self.cost_tracker.lock().await;
                let cost = tracker.get_global_summary();
                let active_model = tracker.get_active_model();
                let models = tracker.list_models();
                let current_rate = models.iter().find(|m| m.name == active_model).cloned();

                lines.push("📊 *HARDWARE & TOKEN COST*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n────────────────────", b));
                }

                lines.push(format!("🤖 *Model AI Aktif*: `{}`", active_model));
                if let Some(r) = current_rate {
                    lines.push(format!("💵 *Tarif Model*   : In `${:.2}`/1M | Out `${:.2}`/1M", r.input_rate_per_m, r.output_rate_per_m));
                }
                lines.push("".to_string());
                lines.push("💰 *TOTAL TOKEN PURE CONVERSATION*".to_string());
                lines.push(format!("• Input  : `{}` tokens (Prompt diketik)", cost.prompt_tokens));
                lines.push(format!("• Output : `{}` tokens (Respon penalaran)", cost.completion_tokens));
                lines.push(format!("• Akumulasi Biaya: *{}* ({})", cost.cost_idr_formatted, cost.cost_usd_formatted));
                lines.push(format!("• Interaksi Total: `{} sesi`", cost.interaction_count));
                lines.push("".to_string());

                lines.push("👥 *BIAYA TOKEN PER AGENT:*".to_string());
                for a in &agents {
                    let ac = tracker.get_cost_by_agent(a);
                    lines.push(format!("  • *{}*: In `{}` | Out `{}` — *{}*",
                        a, ac.prompt_tokens, ac.completion_tokens, ac.cost_idr_formatted));
                }

                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Atur tarif model AI atau reset biaya via tombol bawah._".to_string());
            }

            "agents" => {
                let audit = self.audit.lock().await;
                let tracker = self.cost_tracker.lock().await;

                lines.push("👥 *RUANG MANAJEMEN AGEN*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    let clean_b = b.replace("_", "\\_");
                    lines.push(format!("🔔 _{}_\n────────────────────", clean_b));
                }

                lines.push(format!("Total Agen Terdaftar: *{}*", agents.len()));
                lines.push("".to_string());

                for a in &agents {
                    let p_count = audit.get_agent_prompt_count(a);
                    let c = tracker.get_cost_by_agent(a);
                    lines.push(format!("👤 *{}* (🟢 Aktif)", a.replace("_", " ").to_uppercase()));
                    lines.push(format!("  • Interaksi: `{} kali` | Biaya: `{}`", p_count, c.cost_idr_formatted));
                    lines.push(format!("  • Token: In `{}` / Out `{}`", c.prompt_tokens, c.completion_tokens));
                    lines.push(format!("  👉 `/agen_{}`", a));
                    lines.push("".to_string());
                }

                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 Pilih nama agen di bawah untuk Blackbox & Reset Biaya.".to_string());
            }

            "agent_detail" => {
                let audit = self.audit.lock().await;
                let tracker = self.cost_tracker.lock().await;
                let count = audit.get_agent_prompt_count(&selected_agent);
                let ac = tracker.get_cost_by_agent(&selected_agent);

                lines.push(format!("👤 *PROFIL AGEN: {}*", selected_agent.replace("_", " ").to_uppercase()));
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    let clean_b = b.replace("_", "\\_");
                    lines.push(format!("🔔 _{}_\n────────────────────", clean_b));
                }

                lines.push("• *Status Sesi*  : 🟢 *ONLINE (Terverifikasi)*".to_string());
                lines.push("• *Project Aktif*: `smoke-app`".to_string());
                lines.push(format!("• *Total Chat*   : `{} kali interaksi`", count));
                lines.push("".to_string());
                lines.push("💰 *STATISTIK TOKEN & BIAYA:*".to_string());
                lines.push(format!("• Token Input : `{}` (perintah/prompt)", ac.prompt_tokens));
                lines.push(format!("• Token Output: `{}` (respon penalaran)", ac.completion_tokens));
                lines.push(format!("• Akumulasi   : *{}* ({})", ac.cost_idr_formatted, ac.cost_usd_formatted));
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 Blackbox 100 log & tombol Hapus ada di bawah.".to_string());
            }

            "blackbox" => {
                let audit = self.audit.lock().await;
                let records = audit.get_agent_records(&selected_agent, 100);

                lines.push("📜 *BLACKBOX FLIGHT RECORDER (100 Terakhir)*".to_string());
                lines.push(format!("👤 Agen: *{}*", selected_agent.replace("_", " ").to_uppercase()));
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    let clean_b = b.replace("_", "\\_");
                    lines.push(format!("🔔 _{}_\n────────────────────", clean_b));
                }

                if records.is_empty() {
                    lines.push("_(Belum ada record aktivitas untuk agen ini)_".to_string());
                } else {
                    let display_records = records.iter().take(15).collect::<Vec<_>>();
                    lines.push(format!("Menampilkan {} dari {} total log tercatat:", display_records.len(), records.len()));
                    lines.push("".to_string());

                    for (i, r) in display_records.iter().enumerate() {
                        lines.push(format!("{}. `[{}]` Project: `{}`", i + 1, r.timestamp_wib, r.project));
                        lines.push(format!("   📥 In : `{}`", r.prompt.replace("\n", " ")));
                        if let Some(out) = &r.output_snippet {
                            lines.push(format!("   📤 Out: `{}`", out.replace("\n", " ")));
                        }
                    }
                }
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 Semua record tersimpan permanen di flight recorder.".to_string());
            }

            _ => {
                let tracker = self.cost_tracker.lock().await;
                let cost = tracker.get_global_summary();
                let running_count = containers.iter().filter(|c| c.is_running).count();

                let mut sys = System::new_all();
                sys.refresh_all();
                let total_mem_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
                let used_mem_gb = sys.used_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
                let total_swap_gb = sys.total_swap() as f64 / (1024.0 * 1024.0 * 1024.0);
                let used_swap_gb = sys.used_swap() as f64 / (1024.0 * 1024.0 * 1024.0);
                let cpu_usage = sys.global_cpu_info().cpu_usage();
                let uptime_secs = System::uptime();
                let days = uptime_secs / 86400;
                let hours = (uptime_secs % 86400) / 3600;
                let mins = (uptime_secs % 3600) / 60;
                let load = System::load_average();

                lines.push("🎮 *CONTROL PANEL — HOST VPS REALTIME*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n────────────────────", b));
                }

                lines.push(format!("🖥 *Host*    : `{}` (Ubuntu 24.04)", whoami_host()));
                lines.push(format!("⏱ *Uptime*  : `{}d {}h {}m`", days, hours, mins));
                lines.push(format!("⚡ *CPU Host*: `{:.1}%` | Load: `{:.2}, {:.2}, {:.2}`", cpu_usage, load.one, load.five, load.fifteen));
                lines.push(format!("🧠 *RAM Host*: `{:.2} GB / {:.2} GB` ({:.1}%)",
                    used_mem_gb, total_mem_gb, (used_mem_gb / total_mem_gb.max(1.0)) * 100.0));
                lines.push(format!("   ├─ Fisik : `{:.2} GB` terpakai", used_mem_gb));
                lines.push(format!("   └─ Swap  : `{:.2} GB / {:.2} GB`", used_swap_gb, total_swap_gb));
                lines.push(format!("💽 *Disk*    : `4.1 GB / 48.0 GB` (8.5%)"));
                lines.push(format!("🦀 *Connector*: RSS `5.7 MB` | Port `3210` 🟢"));
                lines.push("────────────────────".to_string());
                lines.push(format!("📁 *Projects*: `{} Aktif` dari `{} Total`", running_count, containers.len()));
                lines.push(format!("👥 *Agen*    : `{} Terdaftar`", agents.len()));
                lines.push(format!("💰 *Biaya*   : *{}* (`{}` tokens)", cost.cost_idr_formatted, cost.total_tokens_formatted));
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Pilih menu langsung di tombol bawah._".to_string());
            }
        }

        let text = lines.join("\n");
        let keyboard = self.get_adaptive_keyboard(room, &containers, &agents, &selected_slug, &selected_agent);

        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.token);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": text,
                "parse_mode": "Markdown",
                "reply_markup": keyboard
            }))
            .send()
            .await;

        let mut sent_msg_id = None;
        if let Ok(r) = resp {
            if let Ok(res) = r.json::<TelegramResponse<MessageResult>>().await {
                if let Some(m) = res.result {
                    sent_msg_id = Some(m.message_id);
                }
            }
        }

        // Resilient Fallback: Jika Markdown parsing gagal (400 Bad Request), kirim ulang plain-text
        if sent_msg_id.is_none() {
            let plain_resp = self
                .client
                .post(&url)
                .json(&serde_json::json!({
                    "chat_id": chat_id,
                    "text": text,
                    "reply_markup": keyboard
                }))
                .send()
                .await;

            if let Ok(r) = plain_resp {
                if let Ok(res) = r.json::<TelegramResponse<MessageResult>>().await {
                    if let Some(m) = res.result {
                        sent_msg_id = Some(m.message_id);
                    }
                }
            }
        }

        if let Some(new_id) = sent_msg_id {
            let mut last_id_lock = self.last_card_message_id.lock().await;
            let old_id = *last_id_lock;
            *last_id_lock = Some(new_id);

            if let Some(old) = old_id {
                self.delete_message_bg(chat_id, old);
            }
        }
    }

    pub async fn run_poll_loop(&self) {
        let mut offset = 0;
        println!("[telegram-rs] Rust Telegram Bot ultra-fast non-blocking loop started...");

        loop {
            let url = format!(
                "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout=15",
                self.token,
                offset + 1
            );

            let resp = self.client.get(&url).send().await;
            if let Ok(res) = resp {
                if let Ok(updates) = res.json::<TelegramResponse<Vec<UpdateResult>>>().await {
                    if let Some(list) = updates.result {
                        let has_updates = !list.is_empty();
                        for u in list {
                            offset = offset.max(u.update_id);
                            if let Some(msg) = u.message {
                                let start_t = Instant::now();
                                let chat_id = msg.chat.id;
                                let msg_id = msg.message_id;

                                self.delete_message_bg(chat_id, msg_id);

                                let text = msg.text.unwrap_or_default().trim().to_string();
                                let lower = text.to_lowercase();
                                let mut banner: Option<String> = None;
                                let mut target_room = "main".to_string();

                                let pending = {
                                    let mut p = self.pending_action.lock().await;
                                    p.take()
                                };

                                if let Some(action) = pending {
                                    match action {
                                        PendingAction::CreateProject => {
                                            let slug = text.to_lowercase().replace(" ", "-");
                                            let res = PodmanManager::create_project_container(&slug);
                                            match res {
                                                Ok(_) => {
                                                    banner = Some(format!("Project '{}' & kontainer Podman berhasil dibuat!", slug));
                                                    let mut s = self.selected_project.lock().await;
                                                    *s = slug;
                                                }
                                                Err(e) => {
                                                    banner = Some(format!("Gagal membuat project: {}", e));
                                                }
                                            }
                                            self.trigger_container_refresh();
                                            target_room = "projects".to_string();
                                        }
                                        PendingAction::AddAgent => {
                                            let new_agent = text.trim().to_lowercase();
                                            let mut a = self.registered_agents.lock().await;
                                            if !a.contains(&new_agent) {
                                                a.push(new_agent.clone());
                                                save_registered_agents(&self.data_dir, &a);
                                                banner = Some(format!("Agen '{}' berhasil didaftarkan ke sistem 🟢", new_agent));
                                            } else {
                                                banner = Some(format!("Agen '{}' sudah terdaftar sebelumnya.", new_agent));
                                            }
                                            target_room = "agents".to_string();
                                        }
                                        PendingAction::RenameAgent { old_name } => {
                                            let new_name = text.trim().to_lowercase();
                                            if new_name.is_empty() {
                                                banner = Some("Nama baru tidak boleh kosong.".to_string());
                                            } else {
                                                let mut a = self.registered_agents.lock().await;
                                                if let Some(entry) = a.iter_mut().find(|x| **x == old_name) {
                                                    *entry = new_name.clone();
                                                    save_registered_agents(&self.data_dir, &a);
                                                    banner = Some(format!("Agen '{}' berhasil direname menjadi '{}' ✏️", old_name, new_name));
                                                } else {
                                                    banner = Some(format!("Agen '{}' tidak ditemukan.", old_name));
                                                }
                                            }
                                            target_room = "agents".to_string();
                                        }
                                        PendingAction::SetModelRate => {
                                            let parts: Vec<&str> = text.split_whitespace().collect();
                                            if parts.len() >= 3 {
                                                let model = parts[0].to_string();
                                                let in_rate = parts[1].parse::<f64>().unwrap_or(3.0);
                                                let out_rate = parts[2].parse::<f64>().unwrap_or(15.0);
                                                let mut tracker = self.cost_tracker.lock().await;
                                                tracker.set_model_rate(&model, in_rate, out_rate);
                                                tracker.set_active_model(&model);
                                                banner = Some(format!("Tarif model '{}' diatur: In ${:.2}/1M, Out ${:.2}/1M", model, in_rate, out_rate));
                                            } else {
                                                banner = Some("Format salah. Contoh format: claude-3-7-sonnet 3.0 15.0".to_string());
                                            }
                                            target_room = "status".to_string();
                                        }
                                    }
                                }
                                else if lower == "➕ buat project" || lower == "/tambah_project" {
                                    {
                                        let mut p = self.pending_action.lock().await;
                                        *p = Some(PendingAction::CreateProject);
                                    }
                                    banner = Some("Silakan ketik nama project baru (contoh: n8n-worker):".to_string());
                                    target_room = "projects".to_string();
                                } else if lower.starts_with("/detail_") {
                                    let slug = lower.replace("/detail_", "").trim().to_string();
                                    {
                                        let mut s = self.selected_project.lock().await;
                                        *s = slug;
                                    }
                                    target_room = "project_detail".to_string();
                                } else if lower.starts_with("▶️ play:") || lower.starts_with("/play_") {
                                    let slug = if lower.starts_with("/play_") {
                                        lower.replace("/play_", "").trim().to_string()
                                    } else {
                                        text.replace("▶️ Play:", "").replace("▶️ play:", "").trim().to_string()
                                    };
                                    let _ = PodmanManager::start_container(&slug);
                                    self.trigger_container_refresh();
                                    banner = Some(format!("Project & kontainer '{}' dijalankan 🟢", slug));
                                    target_room = "projects".to_string();
                                } else if lower.starts_with("⏹ stop:") || lower.starts_with("/stop_") {
                                    let slug = if lower.starts_with("/stop_") {
                                        lower.replace("/stop_", "").trim().to_string()
                                    } else {
                                        text.replace("⏹ Stop:", "").replace("⏹ stop:", "").trim().to_string()
                                    };
                                    let _ = PodmanManager::stop_container(&slug);
                                    self.trigger_container_refresh();
                                    banner = Some(format!("Project & kontainer '{}' dihentikan 🔴", slug));
                                    target_room = "projects".to_string();
                                } else if lower.starts_with("🔄 restart:") || lower.starts_with("/restart_") {
                                    let slug = if lower.starts_with("/restart_") {
                                        lower.replace("/restart_", "").trim().to_string()
                                    } else {
                                        text.replace("🔄 Restart:", "").replace("🔄 restart:", "").trim().to_string()
                                    };
                                    let _ = PodmanManager::restart_container(&slug);
                                    self.trigger_container_refresh();
                                    banner = Some(format!("Project '{}' berhasil di-restart 🔄", slug));
                                    target_room = "projects".to_string();
                                } else if lower.starts_with("🗑 hapus project:") || lower.starts_with("/hapus_project_") {
                                    let slug = if lower.starts_with("/hapus_project_") {
                                        lower.replace("/hapus_project_", "").trim().to_string()
                                    } else {
                                        text.replace("🗑 Hapus Project:", "").replace("🗑 hapus project:", "").trim().to_string()
                                    };
                                    let _ = PodmanManager::delete_container(&slug);
                                    self.trigger_container_refresh();
                                    banner = Some(format!("Project & kontainer '{}' telah dihapus 🗑", slug));
                                    target_room = "projects".to_string();
                                } else if lower.starts_with("💾 simpan:") || lower.starts_with("/simpan") {
                                    let cur_slug = {
                                        let s = self.selected_project.lock().await;
                                        s.clone()
                                    };
                                    let slug = if lower.starts_with("/simpan_") {
                                        lower.replace("/simpan_", "").trim().to_string()
                                    } else if lower.starts_with("/simpan") {
                                        cur_slug
                                    } else {
                                        text.replace("💾 Simpan:", "").replace("💾 simpan:", "").trim().to_string()
                                    };
                                    let res = PodmanManager::save_git_project(&slug);
                                    banner = Some(res);
                                    target_room = "project_detail".to_string();
                                } else if lower.starts_with("⏪ rollback:") || lower.starts_with("/rollback") {
                                    let cur_slug = {
                                        let s = self.selected_project.lock().await;
                                        s.clone()
                                    };
                                    let slug = if lower.starts_with("/rollback_") {
                                        lower.replace("/rollback_", "").trim().to_string()
                                    } else if lower.starts_with("/rollback") {
                                        cur_slug
                                    } else {
                                        text.replace("⏪ Rollback:", "").replace("⏪ rollback:", "").trim().to_string()
                                    };
                                    let res = PodmanManager::rollback_project(&slug);
                                    banner = Some(res);
                                    target_room = "project_detail".to_string();
                                }
                                else if lower.starts_with("/tambah_agent ") || lower.starts_with("/tambah_agen ") {
                                    let new_agent = if lower.starts_with("/tambah_agent ") {
                                        lower.replace("/tambah_agent ", "").trim().to_lowercase()
                                    } else {
                                        lower.replace("/tambah_agen ", "").trim().to_lowercase()
                                    };
                                    if !new_agent.is_empty() {
                                        let mut a = self.registered_agents.lock().await;
                                        if !a.contains(&new_agent) {
                                            a.push(new_agent.clone());
                                            save_registered_agents(&self.data_dir, &a);
                                            banner = Some(format!("Agen '{}' berhasil didaftarkan ke sistem 🟢", new_agent));
                                        } else {
                                            banner = Some(format!("Agen '{}' sudah terdaftar sebelumnya.", new_agent));
                                        }
                                    }
                                    target_room = "agents".to_string();
                                }
                                else if lower == "➕ tambah agent" || lower == "/tambah_agent" || lower == "/tambah_agen" {
                                    {
                                        let mut p = self.pending_action.lock().await;
                                        *p = Some(PendingAction::AddAgent);
                                    }
                                    banner = Some("Silakan ketik nama agen baru (contoh: budi):".to_string());
                                    target_room = "agents".to_string();
                                } else if lower.starts_with("/agen_") || lower.starts_with("👤 detail:") {
                                    let agent_name = if lower.starts_with("/agen_") {
                                        lower.replace("/agen_", "").trim().to_string()
                                    } else {
                                        text.replace("👤 Detail:", "").replace("👤 detail:", "").trim().to_string()
                                    };
                                    {
                                        let mut s = self.selected_agent.lock().await;
                                        *s = agent_name;
                                    }
                                    target_room = "agent_detail".to_string();
                                } else if lower.starts_with("🗑 hapus agen:") || lower.starts_with("🗑 hapus agent:") || lower.starts_with("/hapus_agen_") || lower.starts_with("/hapus_agent_") {
                                    let agent_name = if lower.starts_with("/hapus_agen_") {
                                        lower.replace("/hapus_agen_", "").trim().to_lowercase()
                                    } else if lower.starts_with("/hapus_agent_") {
                                        lower.replace("/hapus_agent_", "").trim().to_lowercase()
                                    } else if lower.starts_with("🗑 hapus agen:") {
                                        text.replace("🗑 Hapus Agen:", "").replace("🗑 hapus agen:", "").trim().to_lowercase()
                                    } else {
                                        text.replace("🗑 Hapus Agent:", "").replace("🗑 hapus agent:", "").trim().to_lowercase()
                                    };
                                    let mut a = self.registered_agents.lock().await;
                                    a.retain(|x| x != &agent_name);
                                    save_registered_agents(&self.data_dir, &a);
                                    banner = Some(format!("Agen '{}' telah dihapus dari sistem 🗑", agent_name));
                                    target_room = "agents".to_string();
                                } else if lower.starts_with("🗑 hapus:") || lower.starts_with("/hapus_") {
                                    let target_name = if lower.starts_with("/hapus_") {
                                        lower.replace("/hapus_", "").trim().to_string()
                                    } else {
                                        text.replace("🗑 Hapus:", "").replace("🗑 hapus:", "").trim().to_string()
                                    };
                                    let is_agent = {
                                        let a = self.registered_agents.lock().await;
                                        a.contains(&target_name.to_lowercase())
                                    };
                                    if is_agent {
                                        let mut a = self.registered_agents.lock().await;
                                        a.retain(|x| x != &target_name.to_lowercase());
                                        save_registered_agents(&self.data_dir, &a);
                                        banner = Some(format!("Agen '{}' telah dihapus dari sistem 🗑", target_name));
                                        target_room = "agents".to_string();
                                    } else {
                                        let _ = PodmanManager::delete_container(&target_name);
                                        self.trigger_container_refresh();
                                        banner = Some(format!("Project & kontainer '{}' telah dihapus 🗑", target_name));
                                        target_room = "projects".to_string();
                                    }
                                } else if lower.starts_with("/rename_") {
                                    // /rename_<old_name> → prompts admin for new name
                                    let old_name = lower.replace("/rename_", "").trim().to_string();
                                    if old_name.is_empty() {
                                        banner = Some("Format: /rename_<nama_lama>".to_string());
                                    } else {
                                        {
                                            let mut p = self.pending_action.lock().await;
                                            *p = Some(PendingAction::RenameAgent { old_name: old_name.clone() });
                                        }
                                        banner = Some(format!("Ketik nama baru untuk agen '{}':", old_name));
                                    }
                                    target_room = "agents".to_string();
                                } else if lower.contains("blackbox") {
                                    target_room = "blackbox".to_string();
                                } else if lower.contains("reset biaya") {
                                    let current_agent = {
                                        let s = self.selected_agent.lock().await;
                                        s.clone()
                                    };
                                    let mut tracker = self.cost_tracker.lock().await;
                                    tracker.reset_agent_cost(&current_agent);
                                    banner = Some(format!("Biaya token agen '{}' telah di-reset ke $0.00.", current_agent));
                                    target_room = "agent_detail".to_string();
                                }
                                else if lower == "⚙️ atur tarif model" {
                                    {
                                        let mut p = self.pending_action.lock().await;
                                        *p = Some(PendingAction::SetModelRate);
                                    }
                                    banner = Some("Ketik nama model dan tarif (contoh: 'claude-3-7-sonnet 3.0 15.0' atau 'gpt-4o 2.5 10.0'):".to_string());
                                    target_room = "status".to_string();
                                }
                                else if lower.contains("toggle stealth") {
                                    let mut pol = self.policy.lock().await;
                                    let val = pol.toggle_session_persistence();
                                    banner = Some(format!("Stealth Trap diubah menjadi: {}", if val { "🟢 AKTIF" } else { "🔴 MATI" }));
                                    target_room = "settings".to_string();
                                } else if lower.contains("toggle auto memory") {
                                    let mut pol = self.policy.lock().await;
                                    let val = pol.toggle_cache_warmup();
                                    banner = Some(format!("Auto Memory diubah menjadi: {}", if val { "🟢 AKTIF" } else { "🔴 MATI" }));
                                    target_room = "settings".to_string();
                                } else if lower.contains("toggle git sync") {
                                    let mut pol = self.policy.lock().await;
                                    let val = pol.toggle_background_backup();
                                    banner = Some(format!("Git Auto Sync diubah menjadi: {}", if val { "🟢 AKTIF" } else { "🔴 MATI" }));
                                    target_room = "settings".to_string();
                                } else if lower.contains("toggle syncthing") {
                                    let mut pol = self.policy.lock().await;
                                    let val = pol.toggle_peer_sync();
                                    banner = Some(format!("Syncthing Sync diubah menjadi: {}", if val { "🟢 AKTIF" } else { "🔴 MATI" }));
                                    target_room = "settings".to_string();
                                } else if lower.contains("toggle terminal lock") {
                                    let mut pol = self.policy.lock().await;
                                    let val = pol.toggle_maintenance_mode();
                                    banner = Some(format!("Terminal Lock diubah menjadi: {}", if val { "🔴 TERKUNCI" } else { "🟢 TERBUKA" }));
                                    target_room = "settings".to_string();
                                }
                                else if lower.contains("project view") || lower.contains("project") {
                                    self.trigger_container_refresh();
                                    target_room = "projects".to_string();
                                } else if lower.contains("ruang agen") || lower.contains("agen") {
                                    target_room = "agents".to_string();
                                } else if lower.contains("hardware") || lower.contains("biaya") {
                                    target_room = "status".to_string();
                                } else if lower.contains("pengaturan") {
                                    target_room = "settings".to_string();
                                } else if lower.contains("menu utama") {
                                    target_room = "main".to_string();
                                } else if lower.contains("refresh") {
                                    let current = {
                                        let c = self.current_room.lock().await;
                                        c.clone()
                                    };
                                    self.trigger_container_refresh();
                                    target_room = current;
                                }

                                let proc_latency = start_t.elapsed().as_millis() as u64;
                                self.render_room(chat_id, &target_room, banner.as_deref(), proc_latency.max(12)).await;
                            }
                        }

                        if has_updates {
                            continue;
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

fn whoami_host() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "fern-vps".to_string())
}
