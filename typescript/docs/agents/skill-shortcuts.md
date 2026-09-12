# SKILL SHORTCUTS — Router 65 Skill Aktif

> **WAJIB untuk semua agent di repo ini.** Sebelum MENJAWAB apa pun: cocokkan permintaan
> terhadap tabel TRIGGER di bawah (butuh ±2 detik). Cocok = skillnya WAJIB dijalankan
> sesuai `SKILL.md`-nya (file lengkap di `.claude/skills/<nama>/` atau `.agents/skills/<nama>/`).
> Tidak cocok dengan satu pun = barus boleh jawab langsung. Ragu = pakai `ask-matt`.
> Pembaruan: 2026-09-10 · 65 skill aktif · 8 skill pasif ada di `CLAUDE.md`.

**Format:** `TRIGGER (apa yang user bilang/situasi) → SKILL — deskripsi jelas (deliverable)`

---

## 0. Router dari router

| Trigger | Skill | Deskripsi |
|---|---|---|
| "skill mana yang cocok?" / ragu total | `ask-matt` | Rutekan situasi Anda ke skill/flow yang tepat dari seluruh koleksi (jawaban: skill mana + urutan) |

## 1. Pikiran, interogasi & keputusan — SEBELUM menulis kode

| Trigger | Skill | Deskripsi |
|---|---|---|
| "grill rencanaku", "tanyain aku sampai tuntas" | `grilling` | Mesin interogasi tak kenal ampun terhadap rencana/keputusan/ide — bongkar asumsi satu per satu |
| `grill-me` (slash) | `grill-me` | Pintu masuk user → menjalankan `grilling` terhadap plan/desain Anda |
| "grill + buat dokumentasinya sekalian" | `grill-with-docs` | Grilling yang sambil jalan MENULIS ADR + glosarium dari keputusan yang tercapai |
| "grill spec workflow ini" | `loop-me` | Grilling khusus spec workflow yang akan dibangun di workspace ini |
| "brainstorm dulu", fitur/komponen baru | `brainstorming` | (superpowers) Wajib sebelum kerja kreatif — gali requirement via pertanyaan Socratic |
| "tunggu, aku nggak ngerti posisi lo" | `wait-what` | Hentikan, petakan kembali posisi pemahaman Anda vs yang agent kira, pitch ulang |
| "aku nggak bisa jawab ini, tanya ke tim" | `to-questionnaire` | Ubah keputusan yang tak terjawab jadi kuesioner rapi untuk diisi orang lain |
| "council ini", "pressure-test keputusan ini" | `llm-council` | Dewan 5 advisor AI (style berbeda) → peer review anonim → chairman: verdict 5 bagian (1x langkah pertama) |

## 2. Spec → Tiket → Implementasi

| Trigger | Skill | Deskripsi |
|---|---|---|
| "jadikan spec", "rapiin diskusi ini jadi spec" | `to-spec` | Sintesis percakapan berjalan jadi spec + publish ke GitHub Issues (tanpa interview) |
| "pecah jadi tiket" | `to-tickets` | Pecah spec/plan jadi set tiket tracer-bullet dengan edge blokir, publish ke tracker |
| "implement spec/tiket #N" | `implement` | Implementasi sebuah pekerjaan dari spec atau set tiket |
| ada spec + tiket tertaut | `implement-spec` | Implementasi spesifikasi yang sudah dikaitkan ke tiket (varian formal `implement`) |
| "TDD", "test dulu", "red-green-refactor" | `tdd` | (mattpocock) TDD: tulis test merah → hijau → refactor, termasuk integration tests |
| fitur/bugfix, sebelum kode implementasi | `test-driven-development` | (superpowers) TDD paksa sebelum menulis kode implementasi apa pun |
| "buat plan-nya" dari spec/requirement | `writing-plans` | (superpowers) Plan implementasi komprehensif, langkah demi langkah, SEBELUM menyentuh kode |
| ada plan tertulis, sesi baru | `executing-plans` | (superpowers) Eksekusi plan di sesi terpisah dengan checkpoint review |
| plan punya tugas-tugas independen | `subagent-driven-development` | (superpowers) Jalankan tiap tugas oleh sub-agent dengan verifikasi per tugas |
| 2+ tugas independen tanpa shared state | `dispatching-parallel-agents` | (superpowers) Dispatch paralel ke beberapa agent, lalu rangkai hasilnya |
| mulai kerja fitur yang butuh isolasi | `using-git-worktrees` | (superpowers) Isolasi workspace via git worktree sebelum eksekusi plan |
| "semua test lulus, gimana lanjutnya?" | `finishing-a-development-branch` | (superpowers) Keputusan integrasi kerja selesai: merge/PR/cleanup yang benar |
| "/ralph-loop ...", "loop tugasku sampai beres" | `ralph-wiggum` | (anthropics) Loop otonom in-session: ulangi prompt yang sama sampai `<promise>` benar-benar tercapai atau max-iterasi — wajib ada verifikasi otomatis (test/lint/build) |

## 3. Kualitas kode, debug & konflik

| Trigger | Skill | Deskripsi |
|---|---|---|
| "review branch/PR/inilah diff-ku" | `code-review` | Review 2 sumbu PARALEL: Standards (patuh docs repo) + Spec (sesuai issue) — laporan berdampingan |
| selesai fitur besar, sebelum merge | `requesting-code-review` | (superpowers) Minta review: verifikasi kerja memenuhi requirement |
| dapat feedback review, ragu/klarifikasi | `receiving-code-review` | (superpowers) Verifikasi teknis tiap saran SEBELUM mengimplementasi — jangan telan mentah |
| mau klaim "selesai/diperbaiki/lewat" | `verification-before-completion` | (superpowers) Gate bukti: jalankan verifikasi nyata dulu, baru boleh klaim |
| ada bug/failure, sebelum usul fix | `systematic-debugging` | (superpowers) 4 tahap root-cause (repro → isolasi → hipotesis → fix) — dilarang fix tebak-tebakan |
| "diagnose ini", "lambat/regresi" | `diagnosing-bugs` | Loop diagnosis bug sulit & regresi performa; semua output di-reedaksi dari secrets |
| "konflik merge/rebase-nya" | `resolving-merge-conflicts` | Resolusi konflik in-progres: pahami intent kedua sisi, jangan asal pilih |

## 4. Arsitektur & domain

| Trigger | Skill | Deskripsi |
|---|---|---|
| istilah domain kabur, "apa nama resmi X" | `domain-modeling` | Bangun/asah model domain: tulis/edit `CONTEXT.md`, glosarium, dan ADR |
| "arsitektur kita bisa diperdalam?" | `improve-codebase-architecture` | Scan peluang pendalaman modul → laporan HTML visual → grill yang Anda pilih |
| "prototipe dulu biar kerasa" | `prototype` | Prototipe buang-jadi untuk menjawab pertanyaan desain (state model/logika/UI) |

## 5. Memahami codebase — Understand-Anything (graph + dashboard)

| Trigger | Skill | Deskripsi |
|---|---|---|
| "analisis codebase ini", `/understand` | `understand` | Pipeline 5 subagent → knowledge graph (`knowledge-graph.json`) semua file/fungsi/kelas/dependensi |
| "tanya soal codebase: bagaimana X bekerja?" | `understand-chat` | Tanya apa saja berbasis graph (grounded ke kode nyata, bukan tebakan) |
| "buka dashboard-nya" | `understand-dashboard` | Dashboard web interaktif: layer berwarna, node klik-untuk-lihat-kode, pencarian |
| "apa dampak diff/PR ini?" | `understand-diff` | Analisis komponen terpengaruh + risiko dari git diff/PR |
| "ekstrak domain bisnisnya" | `understand-domain` | Domain flow graph: domain, flow, langkah (standalone atau dari graph yang ada) |
| "jelasin file/fungsi ini" | `understand-explain` | Deep-dive satu file/fungsi/modul dengan konteks relasinya |
| ada file Figma | `understand-figma` | File Figma → design knowledge graph (halaman, komponen, tokens) |
| ada wiki KB (pola Karpathy) | `understand-knowledge` | Wiki → graph entitas + relasi implisit + clustering topik |
| "buat panduan onboarding" | `understand-onboard` | Panduan onboarding komprehensif untuk anggota baru |

## 6. Riset, dokumentasi, tulisan & belajar

| Trigger | Skill | Deskripsi |
|---|---|---|
| "riset topik ini ke sumbernya" | `research` | Riset ke sumber primer high-trust → temuan disimpan sebagai file Markdown di repo |
| "ajarin aku X" | `teach` | Ajarkan skill/konsep baru di dalam workspace ini (belajar bersama) |
| "handoff ke sesi/agent lain" | `handoff` | Kompres percakapan jadi dokumen handoff (state, keputusan, langkah berikutnya) |
| "serahkan ke agent background" | `claude-handoff` | Serahkan ke background agent SEGAR yang langsung meneruskan pekerjaan |
| "retro sesi ini" | `retro` | Retrospektif sesi coding: apa baik/buruk + usulan perbaikan konkret |
| "buat wizard provisioning/credential" | `wizard` | Generate wizard bash interaktif untuk langkah yang hanya bisa dilakukan manusia |
| "buat skill baru untuk proses ini" | `writing-skills` | (superpowers) Membuat skill baru dengan disiplin TDD untuk prosedur berulang |
| menulis: tahap gali ide | `writing-fragments` | *explore*: gali fragmen mentah dari bahan, tanpa struktur |
| menulis: tahap susun alur | `writing-beats` | *exploit I*: rakit fragmen jadi perjalanan beats, tiap istilah ditanamkan duluan |
| menulis: tahap forms article | `writing-shape` | *exploit II*: bentuk bahan jadi artikel per paragraf |

## 7. Keamanan & governance

| Trigger | Skill | Deskripsi |
|---|---|---|
| "scan keamanan repo/inilah URL-nya", "pentest" | `strix` | Jalankan Strix (AI pentest CLI) — mode quick/standard/deep; WAJIB sistem yang Anda kuasai/berwenang |
| "review governance sistem/agent/CI ini" | `runtime-governance-review` | Lensa Strixgov (SGRF v1): report 13 seksi + profil 4 axis (Capability/Governance/Enforcement/Verification) |
| "govern PR/diff ini" | `govern-pr` | Lensa perubahan: sumbu SGRF mana yang digeser PR ini, ke arah mana — verdict siap-komen-PR |
| "siap release?" | `release-readiness` | Lensa rilis: GO / GO-WITH-CONDITIONS / NO-GO + report 13 seksi |
| "wira Strix ke codebase ini" | `strix-wire` | Temukan mutasi tak-terbalikkan (charge/delete/send/migration), bungkus `governedAction()` → evidenceId (butuh runtime Strix) |
| "verifikasi bukti Strix ini" | `strix-verify` | Verifikasi independen artefak Ed25519 (evidence/approval/quorum/receipt) — lokal, offline, tanpa akun |

## 8. Dokumentasi library & prompt

| Trigger | Skill | Deskripsi |
|---|---|---|
| butuh API syntax terkini, "ctx7" | `context7-cli` | CLI `ctx7`: fetch docs library terkini + kelola skill (install/search/suggest/generate) + setup MCP |
| "tulis/perbaiki prompt untuk GPT/Gemini/Midjourney/n8n…" | `prompt-master` | Prompt production-ready per tool: 13 template auto, 20+ profil tool, deteksi 37 pola pembuang kredit |

## 9. Setup & one-shot

| Trigger | Skill | Deskripsi |
|---|---|---|
| ganti issue tracker / setup ulang skill | `setup-matt-pocock-skills` | Konfigurasi repo: tracker, label triage, layout domain docs (sudah dijalankan 2026-09-10 ✓) |
| "tambah pre-commit hook" | `setup-pre-commit` | Husky + lint-staged (Prettier) + typecheck + test di pre-commit |
| repo TypeScript, modul harus dalam | `setup-ts-deep-modules` | dependency-cruiser: tiap package = deep module, implementasi tersembunyi |
| "scaffold latihan soal" | `scaffold-exercises` | Struktur direktori latihan: section, soal, solusi, penjelasan — lolos lint |
| "ganti `as` dengan shoehorn" | `migrate-to-shoehorn` | Migrasi asersi tipe test `as` → @total-typescript/shoehorn |

## 10. Navigasi & triage

| Trigger | Skill | Deskripsi |
|---|---|---|
| "triage issues-nya", issue baru masuk | `triage` | State machine 5 peran (needs-triage→needs-info→ready-for-agent/ready-for-human/wontfix) + brief siap-agent |
| kerja raksasa (>1 sesi agent) | `wayfinder` | Pecah jadi peta tiket keputusan di tracker (map + child tickets + blokir), selesaikan satu-satu |

---

## Alur standar (rakitan cepat)

| Situasi | Rangkaian |
|---|---|
| Fitur baru | `grill-me` → `to-spec` → `to-tickets` → `using-git-worktrees` → `implement` + `tdd` → `verification-before-completion` → `code-review` |
| Bug | `diagnosing-bugs` / `systematic-debugging` → fix via `tdd` → `code-review` |
| Keputusan besar (mahal kalau salah) | `llm-council` (opsi: `grill-with-docs` untuk ADR-nya) |
| PR masuk / akan merge | `code-review` + `govern-pr` (+ `receiving-code-review` bila feedback) |
| Rilis | `release-readiness` → (`strix` bila permukaan berubah) → `strix-verify` bila ada artefak |
| Keamanan rutin | `strix` (scan) → hasil ke `triage` → remedia |
| Butuh context codebase | `mem-search` (riwayat) → `graphify query` → `understand-chat`/`understand-explain` (dalam) |
| Docs/library eksternal | `context7-cli` (verifikasi) — JANGAN andalkan training data |
| Sesinya panjang / ganti sesi | `handoff` atau `claude-handoff` → `retro` di akhir |
| Prompt untuk tool AI lain | `prompt-master` |

## Protokol 2 detik (setiap respons)

1. Baca satu kalimat permintaan user.
2. Goyangkan mental terhadap header 11 kelompok di atas + baris "alur standar".
3. Cocok ≥1 → **jalankan skillnya** (baca `.claude/skills/<nama>/SKILL.md`, ikuti sampai selesai).
4. Cocok beberapa → ikuti "alur standar" untuk situasinya.
5. Nggak cocok apa-apa → jawab langsung. Ragu → `ask-matt`.