# PANDUAN LENGKAP: CONNECTOR-CLI & CONNECTOR SERVER
**Sistem Remote Workspace + Telegram Admin Bot untuk AI Agent**

---

## DAFTAR ISI

1. [Gambaran Sistem](#1-gambaran-sistem)
2. [Sisi Server: Setup di VPS](#2-sisi-server-setup-di-vps)
3. [Sisi Agent: Install Connector-CLI](#3-sisi-agent-install-connector-cli)
4. [Cara Kerja Lengkap untuk Agent](#4-cara-kerja-lengkap-untuk-agent)
5. [Seluruh API Endpoint](#5-seluruh-api-endpoint)
6. [Kontrol via Telegram Bot (Admin)](#6-kontrol-via-telegram-bot-admin)
7. [Alur Kerja Multitasking Agent](#7-alur-kerja-multitasking-agent)
8. [Troubleshooting](#8-troubleshooting)
9. [Environment Variables Lengkap](#9-environment-variables-lengkap)

---

## 1. GAMBARAN SISTEM

```
┌─────────────────────────────────────────────────────┐
│           LAPTOP / MESIN AGENT                      │
│                                                     │
│   AI Agent (Claude, Gemini, GPT, Cursor, dsb)       │
│         │                                           │
│         ▼                                           │
│   connector-cli  (npm -g / atau HTTP langsung)      │
│         │                                           │
└─────────┼───────────────────────────────────────────┘
          │  HTTP ke Port 3210
          │  Header: X-Agent-Name
          ▼
┌─────────────────────────────────────────────────────┐
│           VPS  (fern-vps / 103.55.37.234)           │
│                                                     │
│   connector.service  (Rust Standalone, Port 3210)   │
│   ├── Telegram Admin Bot  (realtime control)        │
│   ├── Policy Manager      (stealth trap, lock, dsb) │
│   ├── Audit / Blackbox    (rekam semua aksi agen)   │
│   ├── Token Cost Tracker  (estimasi biaya LLM)      │
│   ├── Podman Container    (isolasi workspace)       │
│   └── Heritage Memory     (inheritance antar sesi)  │
│                                                     │
│   Data: /var/lib/connector/                         │
│   Binary: /home/fern/connector-server-rs/           │
└─────────────────────────────────────────────────────┘
```

### Apa yang bisa dilakukan sistem ini?

| Fitur | Penjelasan |
|-------|------------|
| **Remote Execution** | Agent jalankan perintah di VPS tanpa SSH langsung |
| **Stealth Trap** | Agent dilarang keluar sandbox (`exit` diblokir otomatis) |
| **Terminal Lock** | Admin bisa membekukan akses terminal agent via Telegram |
| **Audit Blackbox** | Semua prompt & perintah agent dicatat dengan timestamp WIB |
| **Token Cost Tracker** | Estimasi biaya penggunaan token LLM per agent |
| **Podman Container** | Isolasi workload per project container rootless |
| **Git Auto-Sync** | Setiap perubahan project otomatis commit & push |
| **Telegram Bot** | Admin kontrol full dari HP tanpa perlu buka laptop |

---

## 2. SISI SERVER: SETUP DI VPS

### 2.1 Kebutuhan Server

- **OS**: Linux (Ubuntu 22.04 / Debian 12 direkomendasikan)
- **RAM**: Minimal 512MB (aktual Rust server: ~5MB idle)
- **Disk**: Minimal 5GB
- **Tools**: `curl`, `git`, `podman` (opsional, untuk isolasi container)
- **Rust**: Untuk kompilasi dari source

### 2.2 Install Rust di VPS

```bash
# Di VPS sebagai user normal (bukan root)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Verifikasi
cargo --version   # cargo 1.8x.x
```

### 2.3 Clone & Build Connector Server

```bash
# Clone repo
git clone https://github.com/Catzpro01/connector-rust-server.git /home/fern/connector-server-rs
cd /home/fern/connector-server-rs

# Kompilasi release (sekali saja, ~25-60 detik)
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release

# Verifikasi binary ada
ls -lh target/release/connector-server-rs
# -rwxr-xr-x ... 8.2M target/release/connector-server-rs
```

### 2.4 Buat Direktori Data

```bash
sudo mkdir -p /var/lib/connector
sudo chown -R fern:fern /var/lib/connector
```

### 2.5 Buat systemd Service

```bash
sudo nano /etc/systemd/system/connector.service
```

Isi dengan:
```ini
[Unit]
Description=Connector Rust Native Standalone Server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=fern
Group=fern

# Variabel wajib
Environment=PORT=3210
Environment=CONNECTOR_DATA_DIR=/var/lib/connector

# Telegram Bot (ganti token dengan token bot kamu)
Environment=ENABLE_TELEGRAM_BOT=1
Environment=TELEGRAM_BOT_TOKEN=ISI_TOKEN_BOT_TELEGRAM_KAMU

WorkingDirectory=/home/fern/connector-server-rs
ExecStart=/home/fern/connector-server-rs/target/release/connector-server-rs
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
```

```bash
# Aktifkan dan jalankan
sudo systemctl daemon-reload
sudo systemctl enable connector.service
sudo systemctl start connector.service

# Cek status
sudo systemctl status connector.service
```

Output yang diharapkan:
```
● connector.service - Connector Rust Native Standalone Server
     Active: active (running) since ...
     Memory: 5.3M
```

### 2.6 Verifikasi Server Berjalan

```bash
curl http://localhost:3210/health
# {"status":"ok","engine":"rust-native-standalone","version":"0.1.0","active_skills":0,"memory_mb":8.5}

curl http://localhost:3210/api/policy
# {"stealth_trap":true,"auto_import_memory":true,"auto_git_sync":true,"syncthing_sync":false,"terminal_locked":false}
```

### 2.7 Setup Firewall (opsional)

```bash
# Buka port 3210 hanya jika agent perlu akses dari luar
sudo ufw allow 3210/tcp

# Atau jika pakai Nginx reverse proxy (lebih aman):
# Biarkan port 3210 tertutup, akses hanya lewat Nginx proxy ke localhost:3210
```

### 2.8 Update Server (jika ada kode baru)

```bash
cd /home/fern/connector-server-rs
export PATH="$HOME/.cargo/bin:$PATH"

# Pull kode terbaru dari GitHub
git pull origin master

# Kompilasi ulang
cargo build --release

# Restart service
sudo systemctl restart connector.service
```

---

## 3. SISI AGENT: INSTALL CONNECTOR-CLI

Agent bisa berinteraksi dengan connector server melalui **dua cara**:

### Cara A: HTTP Request Langsung (Paling Simpel)

Agent (Claude Code, Cursor, atau agen otonom lainnya) cukup kirim HTTP request ke server tanpa install apapun.

```bash
# Tes koneksi
curl http://VPS_IP:3210/health

# Jalankan perintah di VPS
curl -X POST http://VPS_IP:3210/api/shell/exec \
  -H "Content-Type: application/json" \
  -d '{
    "command": "ls -la /var/lib/connector",
    "agent": "nama_agent_kamu",
    "project": "nama_project"
  }'
```

### Cara B: Install CLI Node.js (dari TypeScript reference)

```bash
# Masuk ke direktori TypeScript reference yang ada di repo
cd /path/ke/connector-rust-server/typescript

# Install dependencies
npm install

# Build CLI
npm run build

# Install global (opsional, supaya bisa dipanggil dari mana saja)
npm install -g .

# Jalankan
connector-cli
```

### Cara C: Setup `.mcp.json` untuk AI Agent (Claude Code / Cursor)

Buat file `.mcp.json` di root project workspace agent:

```json
{
  "servers": {
    "connector": {
      "url": "http://103.55.37.234:3210",
      "transport": "http",
      "headers": {
        "X-Agent-Name": "nama_agent_kamu"
      }
    }
  }
}
```

Ini memungkinkan AI agent (Claude Code, Cursor, dsb) otomatis mendeteksi dan menggunakan connector server sebagai MCP tool.

### 3.1 Konfigurasi Agent (File Config Lokal)

Buat file `~/.connector-cli/config.json` di mesin agent:

```json
{
  "serverUrl": "http://103.55.37.234:3210",
  "agentName": "nama_agent_kamu",
  "githubToken": "ghp_xxxxxxxxxxxxxxxx",
  "defaultProject": "nama_project_default"
}
```

---

## 4. CARA KERJA LENGKAP UNTUK AGENT

### 4.1 Cek Koneksi & Status Server

```bash
# Health check
curl http://VPS_IP:3210/health

# Cek status policy aktif
curl http://VPS_IP:3210/api/policy

# Cek estimasi biaya token
curl http://VPS_IP:3210/api/cost
```

### 4.2 Eksekusi Perintah di VPS

```bash
# Jalankan command di VPS (sinkronus)
curl -X POST http://VPS_IP:3210/api/shell/exec \
  -H "Content-Type: application/json" \
  -d '{
    "command": "git status",
    "agent": "alex",
    "project": "smoke-app",
    "cwd": "/home/fern/projects/smoke-app"
  }'
```

**Response:**
```json
{
  "success": true,
  "exit_code": 0,
  "stdout": "On branch main\nnothing to commit...",
  "stderr": ""
}
```

> **Penting**: Jika `terminal_locked: true` di policy, command akan ditolak dengan `403 Forbidden`.

> **Stealth Trap**: Jika `stealth_trap: true`, perintah `exit` atau `logout` tidak akan benar-benar keluar — agent tetap terjebak di dalam sandbox.

### 4.3 Rekam Aksi ke Blackbox Audit

```bash
curl -X POST http://VPS_IP:3210/api/audit/log \
  -H "Content-Type: application/json" \
  -d '{
    "agent": "alex",
    "project": "smoke-app",
    "prompt": "Deploy container smoke-app ke production",
    "output": "Container berhasil dijalankan di port 8080"
  }'
```

### 4.4 Baca Riwayat Audit

```bash
# Lihat 10 rekaman terbaru
curl http://VPS_IP:3210/api/audit/records?limit=10

# Filter per agent
curl "http://VPS_IP:3210/api/audit/records?agent=alex&limit=5"
```

### 4.5 Swarming: Kunci File (Anti-Konflik Multi-Agent)

```bash
# Kunci file sebelum modifikasi
curl -X POST http://VPS_IP:3210/api/swarming/lock \
  -H "Content-Type: application/json" \
  -d '{
    "project": "smoke-app",
    "file_path": "src/main.rs",
    "agent": "alex",
    "lease_minutes": 10
  }'

# Cek semua lock aktif
curl http://VPS_IP:3210/api/swarming/locks
```

### 4.6 Evolution: Daftarkan Skill Baru

```bash
curl -X POST http://VPS_IP:3210/api/evolution/synthesize \
  -H "Content-Type: application/json" \
  -d '{
    "name": "deploy-smoke",
    "description": "Script deploy container smoke-app ke production",
    "script": "podman stop connector-smoke-app; podman start connector-smoke-app",
    "interpreter": "bash"
  }'

# Lihat semua skill terdaftar
curl http://VPS_IP:3210/api/evolution/skills
```

### 4.7 Lint TypeScript

```bash
curl -X POST http://VPS_IP:3210/api/lint/mattpocock \
  -H "Content-Type: application/json" \
  -d '{"code": "const x: any = {};"}'

# Response:
# [{"line":1,"rule":"MATTPOCOCK-001: Zero Any Policy","snippet":"...","suggestion":"..."}]
```

---

## 5. SELURUH API ENDPOINT

| Method | Endpoint | Fungsi |
|--------|----------|--------|
| `GET` | `/health` | Health check server |
| `GET` | `/api/cost` | Ringkasan biaya token LLM global |
| `GET` | `/api/policy` | Baca status policy aktif |
| `POST` | `/api/policy` | Toggle policy |
| `POST` | `/api/shell/exec` | Eksekusi command di VPS |
| `POST` | `/api/audit/log` | Rekam aksi agent ke blackbox |
| `GET` | `/api/audit/records` | Baca riwayat audit (`?agent=alex&limit=10`) |
| `POST` | `/api/swarming/lock` | Kunci file untuk agent |
| `GET` | `/api/swarming/locks` | Lihat semua file lock aktif |
| `POST` | `/api/evolution/synthesize` | Daftarkan script skill baru |
| `GET` | `/api/evolution/skills` | Lihat semua skill terdaftar |
| `POST` | `/api/lint/mattpocock` | Lint kode TypeScript |

### Policy Toggles (`/api/policy` POST Body)

```json
{"toggle": "stealth_trap"}       // Toggle jebakan exit agent
{"toggle": "auto_import_memory"} // Toggle auto-import knowledge graph
{"toggle": "auto_git_sync"}      // Toggle git auto-commit & push
{"toggle": "syncthing_sync"}     // Toggle Syncthing P2P sync
{"toggle": "terminal_locked"}    // Toggle kunci terminal seluruh agent
```

---

## 6. KONTROL VIA TELEGRAM BOT (ADMIN)

### 6.1 Setup Bot Telegram

1. Buka [@BotFather](https://t.me/BotFather) di Telegram
2. Kirim `/newbot` → ikuti instruksi → salin **token bot**
3. Masukkan token ke environment variable di `connector.service`:
   ```
   Environment=TELEGRAM_BOT_TOKEN=TOKEN_DARI_BOTFATHER
   ```
4. Restart service: `sudo systemctl restart connector.service`
5. Buka bot kamu di Telegram, kirim pesan apapun → bot mendaftar `chat_id` kamu otomatis

### 6.2 Tombol Menu Telegram

**Menu Utama — Control Panel:**

| Tombol | Fungsi |
|--------|--------|
| `🎮 Menu Utama` | Kembali ke control panel utama |
| `🐳 Podman` | Lihat & kelola container Podman |
| `👥 Ruang Agen` | Monitor semua agent & track record |
| `📊 Hardware & Biaya` | Disk, RAM, CPU, estimasi biaya token |
| `⚙️ Pengaturan` | Toggle policy langsung |
| `🐙 GitHub` | Status & update token GitHub |
| `📜 Blackbox` | Riwayat audit semua aksi agent |
| `🔄 Refresh` | Refresh data halaman saat ini |

**Kontrol Container Podman:**

| Tombol | Fungsi |
|--------|--------|
| `⏹ Stop: nama-container` | Hentikan container |
| `▶️ Start: nama-container` | Jalankan container |
| `🔄 Restart: nama-container` | Restart container |
| `🗑 Hapus: nama-container` | Hapus container permanent |
| `⏪ Rollback: nama-container` | Git reset --hard workspace |

**Toggle Pengaturan:**

| Tombol | Fungsi |
|--------|--------|
| `🥷 Toggle Stealth Trap` | ON/OFF jebakan exit agent |
| `🧠 Toggle Auto Memory` | ON/OFF auto-import knowledge graph |
| `📦 Toggle Git Sync` | ON/OFF auto git commit & push |
| `🔄 Toggle Syncthing` | ON/OFF Syncthing P2P sync |
| `🔒 Toggle Terminal Lock` | **Kunci/buka semua terminal agent** |

**Manajemen Agent:**

| Tombol | Fungsi |
|--------|--------|
| `👤 Detail: alex` | Lihat profil, track record, token burn agent alex |
| `👤 Detail: asep` | Lihat profil agent asep |
| `⛔ Kick Sesi: alex` | Putuskan sesi agent alex |

**Simpan Project via Chat:**

```
/simpan                    # simpan & push project aktif (container pertama)
/simpan_nama-project       # simpan project tertentu dengan nama slug
```

### 6.3 Cara Ganti Token GitHub via Telegram

1. Masuk ke menu `🐙 GitHub`
2. Langsung ketik atau paste token baru di kolom chat (format `ghp_xxxx` atau `github_pat_xxxx`)
3. Bot otomatis verifikasi ke API GitHub dan menyimpan token baru
4. Banner konfirmasi muncul: `✅ Token GitHub terverifikasi! Akun: @Catzpro01`

---

## 7. ALUR KERJA MULTITASKING AGENT

### Skenario: 2 Agent Kerja Paralel di Project yang Sama

```
agent alex                    Connector Server               agent asep
    │                               │                             │
    │── lock src/api.rs (10min) ──►│                             │
    │◄── success: lock granted ─── │                             │
    │                               │◄── lock src/api.rs ─────── │
    │                               │──► failed: locked by alex ─►│
    │                               │                             │
    │── shell/exec: cargo build ──►│                             │
    │◄── {success, stdout, stderr}  │                             │
    │                               │                             │
    │── audit/log: "build done" ──►│                             │
    │                               │                             │
    │                               │◄── shell/exec: npm test ── │
    │                               │──► {success, stdout...} ──►│
    │                               │                             │
    │── audit/log: "deploy done" ─►│                             │
    └                               └                             └
```

### Template Alur Agent (Copy-Paste Siap Pakai)

```python
import requests
import json

BASE = "http://103.55.37.234:3210"
AGENT = "alex"
PROJECT = "smoke-app"

def check_policy():
    r = requests.get(f"{BASE}/api/policy").json()
    if r["terminal_locked"]:
        raise Exception("Terminal dikunci admin! Tunggu izin.")
    return r

def lock_file(file_path, minutes=10):
    r = requests.post(f"{BASE}/api/swarming/lock", json={
        "project": PROJECT, "file_path": file_path,
        "agent": AGENT, "lease_minutes": minutes
    }).json()
    if not r["success"]:
        raise Exception(f"File {file_path} sedang dikunci agent lain!")
    return True

def exec_cmd(command, cwd=None):
    body = {"command": command, "agent": AGENT, "project": PROJECT}
    if cwd: body["cwd"] = cwd
    r = requests.post(f"{BASE}/api/shell/exec", json=body).json()
    if not r["success"]:
        raise Exception(f"Command gagal: {r.get('stderr', r.get('error'))}")
    return r["stdout"]

def log_action(prompt, output=""):
    requests.post(f"{BASE}/api/audit/log", json={
        "agent": AGENT, "project": PROJECT,
        "prompt": prompt, "output": output[:200]
    })

# --- Contoh penggunaan ---
check_policy()
lock_file("src/main.py")
log_action("Mulai modifikasi main.py")

out = exec_cmd("python3 -m pytest tests/ -v")
log_action("pytest selesai", out)

print(out)
```

---

## 8. TROUBLESHOOTING

### Server tidak bisa diakses

```bash
# Cek service berjalan
sudo systemctl status connector.service

# Cek port terbuka
ss -tlnp | grep 3210

# Lihat log
sudo journalctl -u connector.service -n 50 --no-pager
```

### Agent mendapat error 403 Terminal Locked

Admin perlu buka lock via Telegram:
1. Buka bot Telegram
2. Masuk `⚙️ Pengaturan`
3. Ketuk `🔒 Toggle Terminal Lock` → pastikan status berubah ke `🟢 TERBUKA`

Atau via API (jika admin punya akses):
```bash
curl -X POST http://VPS_IP:3210/api/policy \
  -H "Content-Type: application/json" \
  -d '{"toggle": "terminal_locked"}'
```

### Perintah `exit` tidak berfungsi di agent

Ini **fitur Stealth Trap** yang sengaja aktif. Untuk nonaktifkan (admin only):
- Telegram → `⚙️ Pengaturan` → `🥷 Toggle Stealth Trap` → OFF

### Telegram bot tidak merespons

```bash
# Cek token valid
curl "https://api.telegram.org/botTOKEN_KAMU/getMe"

# Restart service
sudo systemctl restart connector.service

# Monitor log real-time
sudo journalctl -u connector.service -f
```

### Kompilasi gagal di VPS

```bash
# Pastikan PATH cargo benar
which cargo || source ~/.cargo/env

# Cek Rust version
rustc --version

# Update Rust jika perlu
rustup update stable

# Bersihkan build cache jika ada masalah
cargo clean && cargo build --release
```

---

## 9. ENVIRONMENT VARIABLES LENGKAP

| Variable | Default | Keterangan |
|----------|---------|------------|
| `PORT` | `3210` | Port HTTP server |
| `CONNECTOR_DATA_DIR` | `/var/lib/connector` | Direktori penyimpan data (audit, policy, token costs) |
| `ENABLE_TELEGRAM_BOT` | `1` | Aktifkan Telegram bot (`0` untuk nonaktifkan) |
| `TELEGRAM_BOT_TOKEN` | *(hardcoded fallback)* | Token bot dari @BotFather |
| `HOSTNAME` | `fern-vps` | Nama host yang tampil di Telegram bot |

---

## CATATAN KEAMANAN

- **Jangan expose port 3210 ke internet publik tanpa filter IP atau autentikasi tambahan.** Lindungi dengan firewall yang hanya mengizinkan IP agent yang dikenal.
- **Token GitHub**: Selalu gunakan environment variable di systemd, atau update via Telegram bot. Token disimpan di `/var/lib/connector/github_auth.json`.
- **Kompilasi ulang** diperlukan setiap kali ada perubahan kode Rust. Binary yang dihasilkan ~8MB, waktu kompilasi ~25-60 detik.
- **Data audit** tersimpan di `/var/lib/connector/audit_records.json` — backup berkala dianjurkan.
