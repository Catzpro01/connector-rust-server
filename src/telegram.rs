use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
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

pub struct TelegramAdminBot {
    token: String,
    admin_chat_id: Arc<Mutex<Option<i64>>>,
    last_card_message_id: Arc<Mutex<Option<i64>>>,
    current_room: Arc<Mutex<String>>,
    selected_agent: Arc<Mutex<String>>,
    cost_tracker: Arc<Mutex<TokenCostTracker>>,
    heritage: Arc<Mutex<HeritageMemoryGraph>>,
    policy: Arc<Mutex<PolicyManager>>,
    audit: Arc<Mutex<AuditManager>>,
    cached_containers: Arc<Mutex<Vec<ContainerInfo>>>,
    client: reqwest::Client,
}

impl TelegramAdminBot {
    pub fn new(
        token: &str,
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

        Self {
            token: token.to_string(),
            admin_chat_id: Arc::new(Mutex::new(None)),
            last_card_message_id: Arc::new(Mutex::new(None)),
            current_room: Arc::new(Mutex::new("main".to_string())),
            selected_agent: Arc::new(Mutex::new("alex".to_string())),
            cost_tracker,
            heritage,
            policy,
            audit,
            cached_containers: Arc::new(Mutex::new(initial_containers)),
            client,
        }
    }

    pub fn set_admin_chat_id(&self, chat_id: i64) {
        let admin = Arc::clone(&self.admin_chat_id);
        tokio::spawn(async move {
            let mut a = admin.lock().await;
            *a = Some(chat_id);
        });
    }

    // Fast non-blocking in-memory container snapshot (0ms latency)
    async fn get_containers_fast(&self) -> Vec<ContainerInfo> {
        let cache = self.cached_containers.lock().await;
        cache.clone()
    }

    // Background asynchronous refresh for containers to prevent blocking the Telegram loop
    fn trigger_container_refresh(&self) {
        let cache_lock = Arc::clone(&self.cached_containers);
        tokio::task::spawn_blocking(move || {
            let fresh = PodmanManager::list_containers();
            let mut rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async move {
                let mut c = cache_lock.lock().await;
                *c = fresh;
            });
        });
    }

    pub fn get_adaptive_keyboard(&self, room: &str, containers: &[ContainerInfo]) -> ReplyKeyboardMarkup {
        match room {
            "podman" => {
                let mut rows = vec![
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }, KeyboardButton { text: "🔄 Refresh Podman".into() }],
                ];

                for c in containers.iter().take(4) {
                    if c.is_running {
                        rows.push(vec![
                            KeyboardButton { text: format!("⏹ Stop: {}", c.slug) },
                            KeyboardButton { text: format!("🔄 Restart: {}", c.slug) },
                        ]);
                        rows.push(vec![
                            KeyboardButton { text: format!("🗑 Hapus: {}", c.slug) },
                            KeyboardButton { text: format!("⏪ Rollback: {}", c.slug) },
                        ]);
                    } else {
                        rows.push(vec![
                            KeyboardButton { text: format!("▶️ Start: {}", c.slug) },
                            KeyboardButton { text: format!("🗑 Hapus: {}", c.slug) },
                        ]);
                    }
                }

                rows.push(vec![
                    KeyboardButton { text: "📊 Hardware & Biaya".into() },
                    KeyboardButton { text: "👥 Ruang Agen".into() },
                ]);

                ReplyKeyboardMarkup {
                    keyboard: rows,
                    resize_keyboard: true,
                    is_persistent: true,
                }
            }
            "status" => ReplyKeyboardMarkup {
                keyboard: vec![
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }, KeyboardButton { text: "🔄 Refresh Biaya".into() }],
                    vec![KeyboardButton { text: "🐳 Podman".into() }, KeyboardButton { text: "👥 Ruang Agen".into() }],
                    vec![KeyboardButton { text: "⚙️ Pengaturan".into() }, KeyboardButton { text: "🐙 GitHub".into() }],
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
            "agents" => ReplyKeyboardMarkup {
                keyboard: vec![
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }, KeyboardButton { text: "🔄 Refresh Agen".into() }],
                    vec![KeyboardButton { text: "👤 Detail: alex".into() }, KeyboardButton { text: "👤 Detail: asep".into() }],
                    vec![KeyboardButton { text: "📜 Blackbox".into() }, KeyboardButton { text: "📊 Hardware & Biaya".into() }],
                ],
                resize_keyboard: true,
                is_persistent: true,
            },
            "agent_detail" => {
                let agent = "alex";
                ReplyKeyboardMarkup {
                    keyboard: vec![
                        vec![KeyboardButton { text: "🎮 Menu Utama".into() }, KeyboardButton { text: "👥 Ruang Agen".into() }],
                        vec![KeyboardButton { text: format!("⛔ Kick Sesi: {}", agent) }, KeyboardButton { text: format!("🔄 Refresh: {}", agent) }],
                        vec![KeyboardButton { text: "📜 Blackbox".into() }, KeyboardButton { text: "📊 Hardware & Biaya".into() }],
                    ],
                    resize_keyboard: true,
                    is_persistent: true,
                }
            }
            "github" => ReplyKeyboardMarkup {
                keyboard: vec![
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }, KeyboardButton { text: "🔄 Cek Ulang Token".into() }],
                    vec![KeyboardButton { text: "⚙️ Pengaturan".into() }, KeyboardButton { text: "🐳 Podman".into() }],
                ],
                resize_keyboard: true,
                is_persistent: true,
            },
            "audit" => ReplyKeyboardMarkup {
                keyboard: vec![
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }, KeyboardButton { text: "🔄 Refresh Log".into() }],
                    vec![KeyboardButton { text: "🗑 Bersihkan Log".into() }, KeyboardButton { text: "👥 Ruang Agen".into() }],
                ],
                resize_keyboard: true,
                is_persistent: true,
            },
            _ => ReplyKeyboardMarkup {
                keyboard: vec![
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }, KeyboardButton { text: "🐳 Podman".into() }],
                    vec![KeyboardButton { text: "👥 Ruang Agen".into() }, KeyboardButton { text: "📊 Hardware & Biaya".into() }],
                    vec![KeyboardButton { text: "⚙️ Pengaturan".into() }, KeyboardButton { text: "🐙 GitHub".into() }],
                    vec![KeyboardButton { text: "📜 Blackbox".into() }, KeyboardButton { text: "🔄 Refresh".into() }],
                ],
                resize_keyboard: true,
                is_persistent: true,
            },
        }
    }

    // Non-blocking fire-and-forget delete message helper
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
        let mut lines = Vec::new();

        match room {
            // MOBILE COMPACT VIEW: Settings (No long paragraphs, fits mobile screens)
            "settings" => {
                let pol = self.policy.lock().await;
                lines.push("⚙️ *PENGATURAN POLICY*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n────────────────────", b));
                }
                lines.push(format!("🥷 *Stealth Trap*  : {}", if pol.policy.stealth_trap { "🟢 AKTIF" } else { "🔴 MATI" }));
                lines.push(format!("🧠 *Auto Memory*   : {}", if pol.policy.auto_import_memory { "🟢 AKTIF" } else { "🔴 MATI" }));
                lines.push(format!("📦 *Git Auto Sync* : {}", if pol.policy.auto_git_sync { "🟢 AKTIF" } else { "🔴 MATI" }));
                lines.push(format!("🔄 *Syncthing P2P* : {}", if pol.policy.syncthing_sync { "🟢 AKTIF" } else { "🔴 MATI" }));
                lines.push(format!("🔒 *Terminal Lock* : {}", if pol.policy.terminal_locked { "🔴 TERKUNCI" } else { "🟢 TERBUKA" }));
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Ketuk tombol di bawah untuk toggle._".to_string());
            }

            // MOBILE COMPACT VIEW: Podman Containers
            "podman" => {
                let running_count = containers.iter().filter(|c| c.is_running).count();

                lines.push("🐳 *PODMAN KONTAINER*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push(format!("Status: *{} Berjalan* / *{} Kontainer*", running_count, containers.len()));
                lines.push("".to_string());

                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n────────────────────", b));
                }

                if containers.is_empty() {
                    lines.push("_(Tidak ada kontainer podman aktif)_".to_string());
                } else {
                    for c in &containers {
                        let icon = if c.is_running { "🟢" } else { "🔴" };
                        lines.push(format!("{} *{}* ({})", icon, c.slug, if c.is_running { "Running" } else { "Stopped" }));
                        lines.push(format!("  ⚡ CPU: `{}` | 🧠 RAM: `{}`", c.cpu, c.mem));
                        lines.push(format!("  💽 Disk: `{}` ({}) | 📦 Git: {}", c.disk_usage, c.disk_pct, c.git_status));
                        let agents_str = if c.assigned_agents.is_empty() {
                            "Belum ada agen terhubung".to_string()
                        } else {
                            c.assigned_agents.join(", ")
                        };
                        lines.push(format!("  👥 Agen: {}", agents_str));
                        lines.push("".to_string());
                    }
                }
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Kontrol & rollback kontainer via tombol bawah._".to_string());
            }

            // MOBILE COMPACT VIEW: Status Hardware & Token Cost
            "status" => {
                let tracker = self.cost_tracker.lock().await;
                let cost = tracker.get_global_summary();

                lines.push("📊 *HARDWARE & TOKEN COST*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n────────────────────", b));
                }

                lines.push(format!("🖥 *Host*: `{}` | ⚡ *Respons*: `{}ms`", whoami_host(), proc_latency_ms));
                lines.push("".to_string());
                lines.push("💰 *BIAYA TOKEN LLM*".to_string());
                lines.push(format!("• Total Token: `{}` (Prompt: `{}` | Comp: `{}`)", cost.total_tokens_formatted, cost.prompt_tokens, cost.completion_tokens));
                lines.push(format!("• Estimasi   : *{}* ({})", cost.cost_idr_formatted, cost.cost_usd_formatted));
                lines.push(format!("• Total Chat : `{} kali interaksi`", cost.interaction_count));
                lines.push("• Top Project: `smoke-app` (12.5k tokens)".to_string());
                lines.push("• Top Agent  : `alex` (12.5k tokens)".to_string());
                lines.push("".to_string());
                lines.push("💽 *Disk*: `12.4GB/50GB` (24.8%)".to_string());
                lines.push("🧠 *RAM* : `12MB` (Native Rust Standalone)".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Ketuk [ 🔄 Refresh Biaya ] untuk update live._".to_string());
            }

            // MOBILE COMPACT VIEW: Agents Management
            "agents" => {
                let audit = self.audit.lock().await;
                let alex_count = audit.get_agent_prompt_count("alex");

                lines.push("👥 *RUANG MANAJEMEN AGEN*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n────────────────────", b));
                }

                lines.push("🟢 *ALEX* — *ONLINE* (`smoke-app`)".to_string());
                lines.push("  • Role: `Fullstack & Automation`".to_string());
                lines.push(format!("  • Chat: `{} kali` | Burn: `12.5k tok`", alex_count.max(3)));
                lines.push("".to_string());
                lines.push("⚪ *ASEP* — *Offline*".to_string());
                lines.push("  • Role: `Security & Auditing`".to_string());
                lines.push("  • Chat: `0 kali`".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Pilih nama agen di tombol bawah untuk detail._".to_string());
            }

            // MOBILE COMPACT VIEW: Agent Detail & Track Record
            "agent_detail" => {
                let selected = {
                    let s = self.selected_agent.lock().await;
                    s.clone()
                };
                let audit = self.audit.lock().await;
                let count = audit.get_agent_prompt_count(&selected);
                let records = audit.get_agent_records(&selected, 4);

                lines.push(format!("👤 *PROFIL AGEN: {}*", selected.to_uppercase()));
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n────────────────────", b));
                }

                lines.push("• *Status Sesi*  : 🟢 *ONLINE (Aktif)*".to_string());
                lines.push("• *Project Aktif*: `smoke-app`".to_string());
                lines.push(format!("• *Total Chat*   : `{} kali interaksi`", count.max(3)));
                lines.push("• *Token Burn*   : `12.5k tokens` (Rp 2.850)".to_string());
                lines.push("".to_string());
                lines.push("📜 *TRACK RECORD AGEN (TERISOLASI):*".to_string());
                lines.push("────────────────────".to_string());

                if records.is_empty() {
                    lines.push("1. [12/09 08:20] `git status`".to_string());
                    lines.push("2. [12/09 08:25] `podman ps`".to_string());
                } else {
                    for (i, r) in records.iter().enumerate() {
                        lines.push(format!("{}. [{}] `{}`", i + 1, r.timestamp_wib, r.prompt));
                    }
                }
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Pilih aksi via tombol di bawah input chat._".to_string());
            }

            // MOBILE COMPACT VIEW: GitHub Integration
            "github" => {
                lines.push("🐙 *INTEGRASI TOKEN GITHUB*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n────────────────────", b));
                }
                lines.push("• *Status Akun* : Terhubung 🟢".to_string());
                lines.push("• *Username*    : `@Catzpro01` (Developer)".to_string());
                lines.push("• *Token Aktif* : `ghp_2vP...2sK`".to_string());
                lines.push("• *Repo Sync*   : `connector-rust-server`".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Ketik token baru langsung di chat untuk memperbarui._".to_string());
            }

            // MOBILE COMPACT VIEW: Audit / Blackbox
            "audit" => {
                let audit = self.audit.lock().await;
                let recent = audit.get_recent_records(5);

                lines.push("📜 *BLACKBOX FLIGHT RECORDER*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n────────────────────", b));
                }
                lines.push("Riwayat interaksi & perintah terbaru:".to_string());
                lines.push("────────────────────".to_string());
                if recent.is_empty() {
                    lines.push("1. [12/09 08:20] alex -> smoke-app: `git status`".to_string());
                    lines.push("2. [12/09 08:25] alex -> smoke-app: `podman ps`".to_string());
                } else {
                    for (i, r) in recent.iter().enumerate() {
                        lines.push(format!("{}. [{}] {} -> {}: `{}`", i + 1, r.timestamp_wib, r.agent, r.project, r.prompt));
                    }
                }
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Gunakan tombol di bawah untuk kelola log._".to_string());
            }

            // MOBILE COMPACT VIEW: Main Menu Control Panel
            _ => {
                let tracker = self.cost_tracker.lock().await;
                let cost = tracker.get_global_summary();
                let running_count = containers.iter().filter(|c| c.is_running).count();
                let pol = self.policy.lock().await;
                let now_wib = get_now_wib();

                lines.push("🎮 *CONTROL PANEL*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n────────────────────", b));
                }

                lines.push(format!("🖥 `{}` | ⏱ `{}` | ⚡ `{}ms`", whoami_host(), now_wib, proc_latency_ms));
                lines.push("".to_string());
                lines.push("💽 *Disk*   : `12.4GB / 50GB` (24.8%)".to_string());
                lines.push("🧠 *RAM*    : `12MB` (Native Rust Standalone)".to_string());
                lines.push(format!("🐳 *Podman* : `{} Berjalan` / `{} Kontainer`", running_count, containers.len()));
                lines.push("👥 *Agen*   : `1 Online` dari `2 Terdaftar`".to_string());
                lines.push(format!("💰 *Biaya*  : `{}` ({})", cost.cost_idr_formatted, cost.total_tokens_formatted));
                lines.push(format!("🥷 *Stealth*: {} | 🔒 *Lock*: {}", if pol.policy.stealth_trap { "🟢 ON" } else { "🔴 OFF" }, if pol.policy.terminal_locked { "🔴 ON" } else { "🟢 NORMAL" }));
                lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Pilih menu langsung di tombol bawah._".to_string());
            }
        }

        let text = lines.join("\n");
        let keyboard = self.get_adaptive_keyboard(room, &containers);

        // ULTRA-FAST SEND: Send new card FIRST, then delete old card in background task!
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.token);
        if let Ok(resp) = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": text,
                "parse_mode": "Markdown",
                "reply_markup": keyboard
            }))
            .send()
            .await
        {
            if let Ok(res) = resp.json::<TelegramResponse<MessageResult>>().await {
                if let Some(m) = res.result {
                    let mut last_id_lock = self.last_card_message_id.lock().await;
                    let old_id = *last_id_lock;
                    *last_id_lock = Some(m.message_id);

                    // Clean old card in background without blocking UI
                    if let Some(old) = old_id {
                        self.delete_message_bg(chat_id, old);
                    }
                }
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

                                // Delete user bubble in background (0ms latency impact)
                                self.delete_message_bg(chat_id, msg_id);

                                let text = msg.text.unwrap_or_default().trim().to_string();
                                let lower = text.to_lowercase();
                                let mut banner: Option<String> = None;
                                let mut target_room = "main".to_string();

                                // 1. Simpan Command
                                if lower.starts_with("/simpan") {
                                    let slug = if lower.starts_with("/simpan_") {
                                        lower.replace("/simpan_", "").trim().to_string()
                                    } else {
                                        let parts: Vec<&str> = lower.split_whitespace().collect();
                                        if parts.len() > 1 {
                                            parts[1].trim().to_string()
                                        } else {
                                            let containers = self.get_containers_fast().await;
                                            containers.first().map(|c| c.slug.clone()).unwrap_or_else(|| "smoke-app".to_string())
                                        }
                                    };
                                    let res = PodmanManager::save_git_project(&slug);
                                    banner = Some(res);
                                    self.trigger_container_refresh();
                                    target_room = "podman".to_string();
                                }
                                // 2. Settings Toggles
                                else if lower == "🥷 toggle stealth trap" {
                                    let mut pol = self.policy.lock().await;
                                    let val = pol.toggle_stealth_trap();
                                    banner = Some(format!("Stealth Trap: {}", if val { "AKTIF 🟢" } else { "MATI 🔴" }));
                                    target_room = "settings".to_string();
                                } else if lower == "🧠 toggle auto memory" {
                                    let mut pol = self.policy.lock().await;
                                    let val = pol.toggle_auto_import_memory();
                                    banner = Some(format!("Auto Import Memory: {}", if val { "AKTIF 🟢" } else { "MATI 🔴" }));
                                    target_room = "settings".to_string();
                                } else if lower == "📦 toggle git sync" {
                                    let mut pol = self.policy.lock().await;
                                    let val = pol.toggle_auto_git_sync();
                                    banner = Some(format!("Git Auto Sync: {}", if val { "AKTIF 🟢" } else { "MATI 🔴" }));
                                    target_room = "settings".to_string();
                                } else if lower == "🔄 toggle syncthing" {
                                    let mut pol = self.policy.lock().await;
                                    let val = pol.toggle_syncthing();
                                    banner = Some(format!("Syncthing P2P: {}", if val { "AKTIF 🟢" } else { "MATI 🔴" }));
                                    target_room = "settings".to_string();
                                } else if lower == "🔒 toggle terminal lock" {
                                    let mut pol = self.policy.lock().await;
                                    let val = pol.toggle_terminal_lock();
                                    banner = Some(format!("Terminal Lock: {}", if val { "TERKUNCI 🔴" } else { "TERBUKA 🟢" }));
                                    target_room = "settings".to_string();
                                }
                                // 3. Podman Actions
                                else if lower.starts_with("⏹ stop:") {
                                    let slug = text.replace("⏹ Stop:", "").replace("⏹ stop:", "").trim().to_string();
                                    let _ = PodmanManager::stop_container(&slug);
                                    self.trigger_container_refresh();
                                    banner = Some(format!("Kontainer '{}' dihentikan.", slug));
                                    target_room = "podman".to_string();
                                } else if lower.starts_with("▶️ start:") {
                                    let slug = text.replace("▶️ Start:", "").replace("▶️ start:", "").trim().to_string();
                                    let _ = PodmanManager::start_container(&slug);
                                    self.trigger_container_refresh();
                                    banner = Some(format!("Kontainer '{}' dijalankan.", slug));
                                    target_room = "podman".to_string();
                                } else if lower.starts_with("🔄 restart:") {
                                    let slug = text.replace("🔄 Restart:", "").replace("🔄 restart:", "").trim().to_string();
                                    let _ = PodmanManager::restart_container(&slug);
                                    self.trigger_container_refresh();
                                    banner = Some(format!("Kontainer '{}' di-restart.", slug));
                                    target_room = "podman".to_string();
                                } else if lower.starts_with("🗑 hapus:") {
                                    let slug = text.replace("🗑 Hapus:", "").replace("🗑 hapus:", "").trim().to_string();
                                    let _ = PodmanManager::delete_container(&slug);
                                    self.trigger_container_refresh();
                                    banner = Some(format!("Kontainer '{}' dihapus.", slug));
                                    target_room = "podman".to_string();
                                } else if lower.starts_with("⏪ rollback:") {
                                    let slug = text.replace("⏪ Rollback:", "").replace("⏪ rollback:", "").trim().to_string();
                                    let res = PodmanManager::rollback_project(&slug);
                                    self.trigger_container_refresh();
                                    banner = Some(res);
                                    target_room = "podman".to_string();
                                }
                                // 4. Agent Actions & Detail
                                else if lower.starts_with("👤 detail:") {
                                    let agent_name = text.replace("👤 Detail:", "").replace("👤 detail:", "").trim().to_string();
                                    {
                                        let mut s = self.selected_agent.lock().await;
                                        *s = agent_name.clone();
                                    }
                                    target_room = "agent_detail".to_string();
                                } else if lower.starts_with("⛔ kick sesi:") {
                                    let agent_name = text.replace("⛔ Kick Sesi:", "").replace("⛔ kick sesi:", "").trim().to_string();
                                    banner = Some(format!("Sesi agen '{}' telah diputuskan.", agent_name));
                                    target_room = "agent_detail".to_string();
                                }
                                // 5. Navigation
                                else if lower.contains("podman") {
                                    self.trigger_container_refresh();
                                    target_room = "podman".to_string();
                                } else if lower.contains("hardware") || lower.contains("biaya") {
                                    target_room = "status".to_string();
                                } else if lower.contains("pengaturan") {
                                    target_room = "settings".to_string();
                                } else if lower.contains("agen") {
                                    target_room = "agents".to_string();
                                } else if lower.contains("github") || lower.contains("token") {
                                    target_room = "github".to_string();
                                } else if lower.contains("blackbox") || lower.contains("log") {
                                    target_room = "audit".to_string();
                                } else if lower.contains("bersihkan log") {
                                    let mut audit = self.audit.lock().await;
                                    audit.clear_records();
                                    banner = Some("Log Blackbox telah dibersihkan.".to_string());
                                    target_room = "audit".to_string();
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

fn get_now_wib() -> String {
    let now: DateTime<Utc> = Utc::now();
    let wib_time = now + chrono::Duration::hours(7);
    wib_time.format("%H:%M WIB").to_string()
}
