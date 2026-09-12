# PRD & ARSITEKTUR KANONIK: CONNECTOR MCP SERVER & AGENT WORKSPACE (V1.0)

Dokumen ini adalah **Product Requirement Document (PRD)** komprehensif yang menyatukan seluruh spesifikasi dari berkas repositori `01a08fcd-63af-747c-9bdc-fce49c93ba0a.patch` (mencakup `SPEC.md`, `DESIGN.md`, `CONTEXT.md`, ADR 0001–0013, tracer-bullet issues 01–14, dan panduan produksi), dipadukan dengan seluruh kebutuhan dan masukan Anda mengenai pemisahan peran antara **Laptop (Agent Client)** dan **VPS (Remote Workspace Host)**.

---

## 1. LATAR BELAKANG & PROBLEM STATEMENT

Dalam ekosistem AI coding saat ini, agen otonom menghadapi tiga kendala fundamental:
1. **Context Window Degradation & Context Rot**: Agen terkunci dalam satu balon sesi chat. Ketika token penuh, sesi harus di-reset dan seluruh pemahaman kode, progress, riwayat kegagalan, dan keputusan arsitektur sebelumnya hilang lenyap.
2. **Keterbatasan Eksekusi Lokal & Blocking Execution**: Agen sering kali harus menjalankan kompilasi besar, build container, atau testing yang memakan waktu lama. Menjalankannya secara sinkron di jendela chat membuat interaksi macet dan rawan *timeout*.
3. **Isolasi Keamanan Host**: Memberikan agen akses penuh langsung ke root server produksi sangat berbahaya jika terjadi kegagalan atau halusinasi perintah destruktif.

### Solusi Utama (The Connector Pattern)
Membangun **Connector MCP Server** di VPS dan **Connector CLI** di laptop/klien agen:
- Agen bekerja di VPS layaknya terhubung via SSH tanpa batasan chat balloon.
- Setiap project memiliki **Ledger (Append-Only JSONL)** dan sistem **Inheritance (Pewarisan Generasi)**: sesi agen boleh putus atau diganti agen lain kapan pun tanpa kehilangan progres.
- Mendukung **Multitasking Tab-Style (Background Tasks)** baik di VPS maupun di laptop agen (Local Tasks).
- Eksekusi terisolasi di VPS menggunakan **Rootless Podman Container** per workspace.
- Milestone `success` otomatis menerbitkan/push riwayat Git ke GitHub.

---

## 2. TIGA TUGAS UTAMA ALAT INI (CORE PILLARS)

Sesuai dengan kebutuhan yang Anda tetapkan, sistem ini berdiri di atas 3 pilar:

### Pilar 1: Pewarisan Kondisi / Progress Agar Tidak Hilang (Session Continuity)
- **Append-Only Ledger**: Setiap aksi agen (`project.create`, `fs.write`, `exec.run`, `task.start`, `milestone`, `error`) langsung dicatat sebagai entri JSONL di `<dataDir>/projects/<slug>/ledger.jsonl`.
- **Generation Tracking**: Setiap kali sesi agen baru masuk via `session.enter`, counter `generation` otomatis bertambah (`gen 1 -> gen 2 -> gen 3`).
- **Condensed Inheritance**: Agen generasi berikutnya langsung menerima ringkasan padat (*condensed memory*) yang berisi:
  - Tujuan project & deskripsi.
  - Ringkasan apa yang telah diselesaikan oleh agen generasi sebelumnya.
  - Task yang masih menggantung/open.
  - Berkas apa saja yang baru dimodifikasi.
  - Skill apa yang digunakan.
  - Kesalahan/error yang pernah terjadi (agar agen baru tidak mengulangi lubang yang sama).
- **Zero Hallucination Context**: Agen tidak memerlukan token jutaan untuk membaca ulang sejarah dari nol; informasi disajikan secara *high-leverage*.

### Pilar 2: Multitasking di Disknya Sendiri dan VPS (Dual-Layer Tabs)
- **Remote Tasks (VPS)**:
  - Agen dapat membuka "Tab" di VPS via `exec.run-background` (contoh: `npm install`, `cargo build`, testing).
  - Perintah berjalan di background container, output dicatat di `<workDir>/.connector-tasks/<id>.log`.
  - Agen bebas mengerjakan berkas lain atau berpindah project; status task dicek kemudian via `exec.attach`.
  - Jika koneksi agent terputus, proses di VPS **tidak mati**.
- **Local Tasks (Laptop / Agent Machine)**:
  - Agen juga memiliki kemampuan multitasking di mesin lokalnya sendiri via Local Disk (`~/.connector-cli/tasks`).
  - Menjalankan command background lokal yang bertahan antar pemanggilan CLI via `connector-cli task run <cmd>`, `list`, `attach`, dan `kill`.

### Pilar 3: Kemudahan Akses Agen & Antarmuka Rapi (Agent Usability & TUI)
- Agen yang hanya memiliki akses shell/command prompt pada disk session terisolasi tetap dapat mengontrol VPS secara penuh.
- Tersedia antarmuka menu rapi dan intuitif:
  ```text
  =======================================================
   login : asep
  -------------------------------------------------------
    1. project latest
    2. new project
    3. project list
    4. new tab (run task background)
    5. tab live (supervisi status multitask)
    6. setting
    7. exit
  =======================================================
  ```
- **Setting Menu**: Memungkinkan pengaturan URL server VPS, API key, dan nama agen langsung disimpan di Local Disk (`~/.connector-cli/config.json`) tanpa perlu mengetik ulang variabel lingkungan yang rumit.

---

## 3. ARSITEKTUR & TOPOLOGI SISTEM

```
┌────────────────────────────────────────────────────────────────────────┐
│                        LAPTOP / AGENT ENVIRONMENT                      │
│                                                                        │
│   ┌──────────────────────┐         ┌───────────────────────────────┐   │
│   │   AI Agent / User    │ ◄─────► │        connector-cli          │   │
│   │ (Claude Code, TUI,   │         │ (Menu, Config, Session Disk,  │   │
│   │  Prompt, Cursor, dll)│         │  Local Tasks, MCP Client)     │   │
│   └──────────┬───────────┘         └───────────────┬───────────────┘   │
│              │                                     │                   │
│              │ Membaca .mcp.json                   │                   │
│              ▼                                     ▼                   │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │ Local Disk: ~/.connector-cli (config.json, tasks/)             │   │
│   │ Session Disk: Temp folder (Skills manifest terisolasi)         │   │
│   └────────────────────────────────┬───────────────────────────────┘   │
└────────────────────────────────────┼───────────────────────────────────┘
                                     │ Streamable HTTP (Port 3210)
                                     │ Header: X-API-Key, X-Agent-Name
                                     ▼
┌────────────────────────────────────────────────────────────────────────┐
│                           VPS HOST (Linux)                             │
│                                                                        │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │ connector.service (systemd)                                    │   │
│   │ Express App + McpServer + StreamableHTTPServerTransport        │   │
│   └────────────────┬───────────────────────────────┬───────────────┘   │
│                    ▼                               ▼                   │
│   ┌────────────────────────────────┐  ┌────────────────────────────┐   │
│   │ Data Store (/var/lib/connector)│  │ Execution Engine (Podman)  │   │
│   │ - projects/index.json          │  │ - container per project    │   │
│   │ - <slug>/ledger.jsonl (Memory) │  │ - CPU/RAM/Timeout limit    │   │
│   │ - <slug>/work/ (Git repository)│  │ - background processes     │   │
│   │ - <slug>/.connector-tasks/     │  │ - isolated from Host OS    │   │
│   └────────────────────────────────┘  └────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 4. GLOSARIUM KANONIK (NORMATIVE VOCABULARY - CONTEXT.MD)

Istilah-istilah berikut adalah standar baku (normative) dalam sistem ini:

| Istilah | Definisi | Istilah yang Dihindari |
| :--- | :--- | :--- |
| **Connector** | Server workspace remote di VPS tempat agen terhubung melalui protokol MCP. | gateway, bridge, proxy, daemon |
| **Workspace** | Direktori kerja terisolasi per project di VPS yang memiliki repository Git, container, dan Ledger tersendiri. | project dir, folder, sandbox |
| **Ledger** | Berkas *append-only* JSONL pencatat kronologis seluruh aktivitas agen (alat bukti kebenaran status project). | log, telemetry, trace |
| **Inheritance** | Tindakan sesi agen baru yang secara otomatis menerima ringkasan memori padat dari generasi sebelumnya saat konek. | memory dump, handoff, context replay |
| **Generation** | Angka urutan sesi agen pada suatu project (dimulai dari 1 dan naik setiap kali ada agen masuk). | session number, epoch, version |
| **Task** | Unit pekerjaan paralel di VPS yang berjalan di latar belakang (background process) dan selamat dari diskoneksi. | job, process, thread |
| **Local Task** | Background task yang berjalan di mesin laptop agen sendiri, tercatat di Local Disk. | sub-process, thread |
| **Local Disk** | Direktori persisten di laptop (`~/.connector-cli`) penyimpan konfigurasi, nama agen, dan daftar Local Task. | cache, home dir |
| **Session Disk** | Direktori sementara di laptop agen untuk menampung *skills* unduhan project, dihapus saat sesi selesai. | workspace, cache |
| **Agent Name** | Nama login identitas agen (tanpa password, misal: `asep`), digunakan untuk melabeli entri Ledger. | username, account |
| **Publish** | Tindakan otomatis mendorong (*push*) seluruh riwayat Git workspace ke repositori GitHub ketika milestone `success`. | deploy, sync, backup |

---

## 5. SKEMA SPESIFIKASI ALAT (18 MCP TOOLS)

Connector MCP Server mengekspos 18 tool yang dapat dipanggil oleh agen AI klien:

### Kategori 1: Project Management
1. `project.list`: Mengambil daftar seluruh project di VPS beserta generasi aktif dan milestone terakhirnya.
2. `project.create`: Membuat project baru (menghasilkan folder workspace, inisialisasi Git, `skills.json`, dan entri index).
3. `project.open`: Membuka project dan menjadikannya default untuk sesi yang sedang aktif.
4. `project.overview`: Melirik ringkasan project lain tanpa perlu berpindah fokus.

### Kategori 2: Session & Memory Continuity
5. `session.enter`: Masuk ke project sebagai generasi baru, mencatat event di Ledger, dan mengembalikan memori Inheritance.
6. `session.inherit`: Membaca memori Inheritance tanpa menaikkan counter generasi.
7. `session.track-record`: Memeriksa detail riwayat pekerjaan agen pada generasi tertentu (file apa yang diubah, milestone, task).
8. `session.milestone`: Mencatat pencapaian status (`in-progress`, `success`, `failed`). Status `success` otomatis memicu `git.publish`.

### Kategori 3: Workspace File System (Auto-Commit Git)
9. `fs.read`: Membaca berkas di dalam workspace project VPS.
10. `fs.write`: Menulis berkas di dalam workspace. Setiap penulisan **otomatis di-commit ke Git** dengan identitas agen.
11. `fs.list`: Melihat struktur direktori workspace di VPS.
12. `fs.search`: Pencarian teks di seluruh berkas workspace (otomatis mengabaikan `.git`, `node_modules`, `dist`).

### Kategori 4: Execution Engine & Background Tabs
13. `exec.run`: Menjalankan perintah di container VPS dengan batasan *timeout* dan menangkap stdout/stderr.
14. `exec.run-background`: Menjalankan perintah di background (seperti membuka tab baru) dan mengembalikan Task ID.
15. `exec.attach`: Memeriksa status dan membaca output terbaru dari background Task ID.
16. `exec.list`: Melihat seluruh Task yang sedang berjalan atau sudah selesai di VPS.
17. `exec.kill`: Menghentikan paksa Task background di VPS.

### Kategori 5: GitHub Integration
18. `git.publish`: Mendorong seluruh riwayat Git ke repositori GitHub (membuat repositori private baru secara otomatis jika belum ada).

---

## 6. ALUR PENGGUNAAN LENGKAP BAGI AGEN (AGENT WORKFLOW GUIDE)

Berikut panduan langkah demi langkah saat agen AI yang terisolasi di terminal laptop mulai bekerja menggunakan tool ini:

### Langkah 1: Pemasangan Skill / CLI
Jalankan di terminal laptop:
```bash
# Menginstal CLI secara global
npm install -g connector-mcp-cli
```
*(Atau pasang dari arsip lokal/repo GitHub)*

### Langkah 2: Membuka Menu Interaktif
Jalankan perintah:
```bash
connector-cli
```

### Langkah 3: Login Identitas Agen
- Jika belum pernah login, CLI akan meminta input nama:
  ```text
  Login agent (tanpa password; API key adalah secret): asep
  ```
- Nama `asep` akan disimpan di Local Disk (`~/.connector-cli/config.json`) dan otomatis disematkan pada setiap catatan Ledger di VPS.

### Langkah 4: Tampilan Menu Interaktif
```text
=======================================================
 login : asep
-------------------------------------------------------
  1. project latest
  2. new project
  3. project list
  4. new tab (run task background)
  5. tab live (supervisi status multitask)
  6. setting
  7. exit
=======================================================
Pilih menu (1-7):
```

### Langkah 5: Penjelasan Menu & Tindakan
- **1. project latest**: Melihat project yang terakhir dikerjakan, generasi aktif, milestone terakhir, dan ringkasan context yang diwariskan dari agen sebelumnya.
- **2. new project**: Memasukkan nama project baru untuk dibuatkan workspace, Git repo, dan container di VPS secara otomatis.
- **3. project list**: Melihat daftar semua project di VPS.
- **4. new tab**: Memulai task background (bisa memilih dijalankan di container VPS atau dijalankan di laptop lokal).
- **5. tab live**: Melihat daftar proses multitasking yang sedang berjalan (baik di VPS maupun di laptop).
- **6. setting**: Menyesuaikan link server VPS (`http://...` atau `https://...`), mengganti API Key, atau mengubah nama login agen tanpa harus mengatur file konfigurasi secara manual.

---

## 7. RINGKASAN KEPUTUSAN ARSITEKTUR (ADR 0001 - 0013)

1. **ADR 0001**: Connector adalah *remote workspace server* (bukan gateway proxy) yang memberikan pengalaman layaknya SSH.
2. **ADR 0002**: Isolasi eksekusi menggunakan Rootless Podman Container per workspace demi keamanan host VPS.
3. **ADR 0003**: Model kepemilikan tunggal (Single Owner) dengan multi-agent; semua agen milik owner berbagi akses ke workspace project.
4. **ADR 0004**: Stack pemrograman menggunakan TypeScript monorepo modern (`strict: true`, NodeNext) untuk konsistensi SDK MCP.
5. **ADR 0005**: Kontinuitas sesi dijamin oleh Append-Only Ledger dan sistem Inheritance pada setiap `session.enter`.
6. **ADR 0006**: Kode server Connector dilarang disentuh atau dimodifikasi dari dalam workspace agent demi mencegah eksploitasi.
7. **ADR 0007**: Komunikasi CLI ke VPS menggunakan Streamable HTTP dengan enkripsi HTTPS dan autentikasi berbasis API Key.
8. **ADR 0008**: Setiap operasi file (`fs.write`) otomatis di-commit ke Git workspace agar tidak ada riwayat yang hilang.
9. **ADR 0009**: Penyediaan skills via `skills.json` di workspace yang diunduh ke Session Disk sementara pada laptop agen.
10. **ADR 0010**: Distribusi tunggal (satu repo monorepo mendistribusikan server VPS dan CLI laptop agen).
11. **ADR 0011**: Identitas agen berupa nama login bebas (tanpa password) untuk atribusi label Ledger; API Key adalah rahasia autentikasi.
12. **ADR 0012**: Multitasking lokal didukung lewat Local Tasks pada Local Disk laptop agen.
13. **ADR 0013**: Fungsi Publish otomatis membuat repositori GitHub private baru via GitHub API jika repositori tujuan belum ada.

---

## 8. KESIMPULAN & STATUS IMPLEMENTASI

Sistem ini telah:
1. **Terpasang & Berjalan di VPS (`103.55.37.234`)**: Service `connector.service` aktif di port `3210`, Podman siap, dan repositori terhubung.
2. **Terpasang di Laptop**: CLI `connector-cli` telah terpasang secara global dengan build TypeScript yang bersih (memenuhi standar *Total TypeScript / mattpocock*), menu interaktif telah aktif, dan koneksi ke VPS telah teruji secara live.
3. **Mencegah Hilangnya Rantai Progress**: Dengan pencatatan Ledger dan ringkasan generasi, masalah *context window limit* pada chat session teratasi secara tuntas.
