# connector-cli v2

REST native CLI untuk [connector-server-rs](https://github.com/Catzpro01/connector-rust-server) (Rust).

**Tidak perlu install package external** — hanya butuh Node.js 18+.

## Install (Agent Side)

```bash
# Clone repo sekali
git clone https://github.com/Catzpro01/connector-rust-server.git ~/.connector-server-rs

# Masuk ke folder CLI
cd ~/.connector-server-rs/cli

# Symlink global (sehingga bisa dipanggil sebagai connector-cli dari mana saja)
npm link
```

### Setup URL server

```bash
connector-cli setting set serverUrl http://103.55.37.234:3210
connector-cli setting set agentName alex
connector-cli setting set defaultProject smoke-app
```

Config tersimpan di `~/.connector-cli/config.json`.

## Penggunaan Non-Interaktif (untuk agen otonom)

```bash
# Health
connector-cli health

# Jalankan command di VPS
connector-cli exec "git status" --project smoke-app

# Catat aksi ke blackbox audit
connector-cli audit log "mulai deploy" --project smoke-app

# Kunci file sebelum edit (anti-konflik multi-agent)
connector-cli lock src/main.rs --project smoke-app --minutes 10

# Cek semua lock aktif
connector-cli locks

# Cek policy (terminal_locked, stealth_trap, dsb)
connector-cli policy get

# Toggle policy
connector-cli policy toggle terminal_locked

# Daftar skill
connector-cli skill list

# Tambah skill
connector-cli skill add --name deploy --script "podman restart smoke-app" --desc "Deploy smoke-app"
```

## Penggunaan Interaktif (TUI menu)

```bash
connector-cli
# → Menu interaktif nomor 1-9
```

## Semua Perintah

| Perintah | Fungsi |
|---|---|
| `connector-cli health` | Status server |
| `connector-cli exec "<cmd>"` | Jalankan perintah di VPS |
| `connector-cli audit log "<msg>"` | Rekam ke blackbox |
| `connector-cli audit records` | Lihat riwayat audit |
| `connector-cli lock <file>` | Kunci file |
| `connector-cli locks` | Lihat lock aktif |
| `connector-cli policy get` | Baca policy |
| `connector-cli policy toggle <key>` | Toggle policy |
| `connector-cli skill list` | Daftar skill |
| `connector-cli skill add` | Tambah skill |
| `connector-cli lint "<kode>"` | Lint TypeScript |
| `connector-cli cost` | Ringkasan token cost |
| `connector-cli setting` | Lihat config |
| `connector-cli setting set <k> <v>` | Set config value |

## Kompatibilitas

Didesain untuk **connector-server-rs** (Rust, REST API). **Tidak compatible** dengan connector server versi TypeScript lama (yang memerlukan endpoint `/mcp` MCP JSON-RPC).
