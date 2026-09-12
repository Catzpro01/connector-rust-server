use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cost_tracker::TokenCostTracker;
use crate::heritage::HeritageMemoryGraph;
use crate::podman::PodmanManager;

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
    cost_tracker: Arc<Mutex<TokenCostTracker>>,
    heritage: Arc<Mutex<HeritageMemoryGraph>>,
    client: reqwest::Client,
}

impl TelegramAdminBot {
    pub fn new(
        token: &str,
        cost_tracker: Arc<Mutex<TokenCostTracker>>,
        heritage: Arc<Mutex<HeritageMemoryGraph>>,
    ) -> Self {
        Self {
            token: token.to_string(),
            admin_chat_id: Arc::new(Mutex::new(None)),
            last_card_message_id: Arc::new(Mutex::new(None)),
            current_room: Arc::new(Mutex::new("main".to_string())),
            cost_tracker,
            heritage,
            client: reqwest::Client::new(),
        }
    }

    pub fn set_admin_chat_id(&self, id: i64) {
        let admin_id = Arc::clone(&self.admin_chat_id);
        tokio::spawn(async move {
            let mut lock = admin_id.lock().await;
            *lock = Some(id);
        });
    }

    fn get_adaptive_keyboard(&self, room: &str) -> ReplyKeyboardMarkup {
        match room {
            "podman" => {
                let containers = PodmanManager::list_containers();
                let mut rows = vec![
                    vec![KeyboardButton { text: "🎮 Menu Utama".into() }, KeyboardButton { text: "🔄 Refresh Podman".into() }],
                ];

                for c in containers.iter().take(3) {
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

    pub async fn delete_message(&self, chat_id: i64, message_id: i64) {
        let url = format!("https://api.telegram.org/bot{}/deleteMessage", self.token);
        let _ = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "message_id": message_id
            }))
            .send()
            .await;
    }

    pub async fn render_room(&self, chat_id: i64, room: &str, banner: Option<&str>) {
        {
            let mut r = self.current_room.lock().await;
            *r = room.to_string();
        }

        let mut lines = Vec::new();

        match room {
            "status" => {
                let tracker = self.cost_tracker.lock().await;
                let cost = tracker.get_global_summary();

                lines.push("📊 *STATUS HARDWARE & TOKEN COST (RUST NATIVE)*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n-------------------------------------", b));
                }

                lines.push(format!("🖥 *Host*: `{}` | ⚡ *Engine*: `Rust v1.98`", whoami_host()));
                lines.push("".to_string());
                lines.push("💰 *TOKEN BURN & ESTIMASI BIAYA LLM*".to_string());
                lines.push(format!("• Total Token   : `{}` (Prompt: `{}` | Comp: `{}`)", cost.total_tokens_formatted, cost.prompt_tokens, cost.completion_tokens));
                lines.push(format!("• Estimasi Biaya: *{}* ({})", cost.cost_idr_formatted, cost.cost_usd_formatted));
                lines.push(format!("• Total Interaksi: `{} kali`", cost.interaction_count));
                lines.push("━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Ketuk [ 🔄 Refresh Biaya ] untuk update live._".to_string());
            }
            "podman" => {
                let containers = PodmanManager::list_containers();
                let running_count = containers.iter().filter(|c| c.is_running).count();

                lines.push("🐳 *KELOLA PODMAN KONTAINER (RUST)*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push(format!("Status: *{} Berjalan* dari *{} Kontainer*\n", running_count, containers.len()));

                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n-------------------------------------", b));
                }

                for c in &containers {
                    let icon = if c.is_running { "🟢" } else { "🔴" };
                    lines.push(format!("{} *{}* ({})", icon, c.slug, if c.is_running { "Running" } else { "Stopped" }));
                    lines.push(format!("  ⚡ CPU  : `{}`", c.cpu));
                    lines.push(format!("  🧠 RAM  : `{}`", c.mem));
                    lines.push("".to_string());
                }
                lines.push("━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Kontrol kontainer langsung dari tombol bawah chat._".to_string());
            }
            _ => {
                let tracker = self.cost_tracker.lock().await;
                let cost = tracker.get_global_summary();
                let containers = PodmanManager::list_containers();
                let running_count = containers.iter().filter(|c| c.is_running).count();

                lines.push("🎮 *MENU UTAMA CONTROL PANEL (RUST SERVER)*".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string());
                if let Some(b) = banner {
                    lines.push(format!("🔔 _{}_\n-------------------------------------", b));
                }

                lines.push(format!("🖥 *Host*: `{}` | 🦀 *Runtime*: `Rust Standalone ELF`", whoami_host()));
                lines.push("".to_string());
                lines.push(format!("🐳 *Podman* : `{} Berjalan` / `{} Kontainer`", running_count, containers.len()));
                lines.push(format!("💰 *Biaya*  : `{}` ({})", cost.cost_idr_formatted, cost.total_tokens_formatted));
                lines.push("🥷 *Stealth*: 🟢 ON (Native Single Binary, Zero Node.js)".to_string());
                lines.push("━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string());
                lines.push("💡 _Pilih menu langsung menggunakan tombol di bawah input teks._".to_string());
            }
        }

        let text = lines.join("\n");
        let keyboard = self.get_adaptive_keyboard(room);

        // Atomic 1-Card Replace: Delete old card -> Send new card with adaptive keyboard
        let mut last_id_lock = self.last_card_message_id.lock().await;
        if let Some(old_id) = *last_id_lock {
            self.delete_message(chat_id, old_id).await;
        }

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
                    *last_id_lock = Some(m.message_id);
                }
            }
        }
    }

    pub async fn run_poll_loop(&self) {
        let mut offset = 0;
        println!("[telegram-rs] Rust Telegram Bot polling loop started...");

        loop {
            let url = format!(
                "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout=20",
                self.token,
                offset + 1
            );

            if let Ok(resp) = self.client.get(&url).send().await {
                if let Ok(updates) = resp.json::<TelegramResponse<Vec<UpdateResult>>>().await {
                    if let Some(list) = updates.result {
                        for u in list {
                            offset = offset.max(u.update_id);
                            if let Some(msg) = u.message {
                                let chat_id = msg.chat.id;
                                let msg_id = msg.message_id;

                                // Always delete user message immediately for spotless chat
                                self.delete_message(chat_id, msg_id).await;

                                let text = msg.text.unwrap_or_default().trim().to_lowercase();
                                let mut target_room = "main".to_string();

                                if text.contains("podman") {
                                    target_room = "podman".to_string();
                                } else if text.contains("hardware") || text.contains("biaya") {
                                    target_room = "status".to_string();
                                } else if text.contains("pengaturan") {
                                    target_room = "settings".to_string();
                                } else if text.contains("agen") {
                                    target_room = "agents".to_string();
                                } else if text.starts_with("⏹ stop:") {
                                    let slug = text.replace("⏹ stop:", "").trim().to_string();
                                    let _ = PodmanManager::stop_container(&slug);
                                    target_room = "podman".to_string();
                                } else if text.starts_with("▶️ start:") {
                                    let slug = text.replace("▶️ start:", "").trim().to_string();
                                    let _ = PodmanManager::start_container(&slug);
                                    target_room = "podman".to_string();
                                } else if text.starts_with("🔄 restart:") {
                                    let slug = text.replace("🔄 restart:", "").trim().to_string();
                                    let _ = PodmanManager::restart_container(&slug);
                                    target_room = "podman".to_string();
                                } else if text.starts_with("🗑 hapus:") {
                                    let slug = text.replace("🗑 hapus:", "").trim().to_string();
                                    let _ = PodmanManager::delete_container(&slug);
                                    target_room = "podman".to_string();
                                }

                                self.render_room(chat_id, &target_room, None).await;
                            }
                        }
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
        }
    }
}

fn whoami_host() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "fern-vps".to_string())
}
