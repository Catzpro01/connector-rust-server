!/usr/bin/env node
// connector-cli v2 — REST native client untuk connector-server-rs
// Install: git clone ... && cd cli && npm link
// Usage:   connector-cli [command] [options]
//          connector-cli  (tanpa argumen = mode TUI interaktif)

import { loadConfig, saveConfig, ensureAgentName } from './lib/config.js';
import { ConnectorAPI } from './lib/api.js';
import { interactiveMenu, sep } from './lib/tui.js';

const args = process.argv.slice(2);
const cmd = args[0];

async function main() {
  // Jika tidak ada argumen → mode TUI interaktif
  if (!cmd || cmd === 'menu') {
    const cfg = loadConfig();
    // Jika agentName belum ada → tanya dulu
    if (!cfg.agentName) cfg.agentName = await ensureAgentName();
    const api = new ConnectorAPI(cfg);
    await interactiveMenu(api, cfg, (updates) => saveConfig(updates));
    return;
  }

  const cfg = loadConfig();
  const api = new ConnectorAPI(cfg);

  switch (cmd) {

    // ── Health ───────────────────────────────────────────────────
    case 'health': {
      const r = await api.health();
      console.log(JSON.stringify(r, null, 2));
      break;
    }

    // ── Exec ─────────────────────────────────────────────────────
    case 'exec': {
      const command = args[1];
      if (!command) { console.error('Usage: connector-cli exec "<command>" [--project <p>] [--cwd <dir>]'); process.exit(1); }
      const project = argVal(args, '--project') || argVal(args, '-p') || cfg.defaultProject;
      const cwd = argVal(args, '--cwd') || argVal(args, '-d');
      const r = await api.shellExec(command, { project, cwd });
      if (r.success) {
        if (r.stdout) process.stdout.write(r.stdout);
        if (r.stderr) process.stderr.write(r.stderr);
        process.exit(r.exit_code || 0);
      } else {
        if (r.stderr) process.stderr.write(r.stderr);
        if (r.error) console.error('ERROR:', r.error);
        process.exit(r.exit_code || 1);
      }
      break;
    }

    // ── Audit ────────────────────────────────────────────────────
    case 'audit': {
      const sub = args[1];
      if (sub === 'log') {
        const msg = args[2];
        if (!msg) { console.error('Usage: connector-cli audit log "<pesan>"'); process.exit(1); }
        const project = argVal(args, '--project') || cfg.defaultProject;
        const output = argVal(args, '--output');
        await api.auditLog(msg, output, project);
        console.log('✅ Logged');
      } else if (sub === 'records' || sub === 'list') {
        const agent = argVal(args, '--agent') || argVal(args, '-a');
        const limit = parseInt(argVal(args, '--limit') || argVal(args, '-n') || '20') || 20;
        const records = await api.auditRecords({ agent, limit });
        for (const r of records) {
          console.log(`[${r.timestamp_wib}] ${r.agent}/${r.project || '-'}: ${(r.prompt || '').slice(0, 120)}`);
        }
      } else {
        console.error('Usage: connector-cli audit log "<msg>" | connector-cli audit records [--agent <name>] [--limit <n>]');
      }
      break;
    }

    // ── Lock ─────────────────────────────────────────────────────
    case 'lock': {
      const filePath = args[1];
      if (!filePath) { console.error('Usage: connector-cli lock <file_path> [--project <p>] [--minutes <n>]'); process.exit(1); }
      const project = argVal(args, '--project') || cfg.defaultProject || 'default';
      const minutes = parseInt(argVal(args, '--minutes') || argVal(args, '-m') || '30') || 30;
      const r = await api.lockFile({ file_path: filePath, project, lease_minutes: minutes });
      if (r.success) console.log(`✅ Locked: ${filePath} [${project}]`);
      else { console.error(`❌ Gagal: file sudah dikunci agent lain`); process.exit(1); }
      break;
    }

    case 'locks': {
      const locks = await api.listLocks();
      if (!locks.length) { console.log('(tidak ada lock aktif)'); break; }
      for (const l of locks) {
        console.log(`${l.file_path}  project=${l.project}  agent=${l.agent}  exp=${l.expires_at || 'n/a'}`);
      }
      break;
    }

    // ── Policy ────────────────────────────────────────────────────
    case 'policy': {
      const sub = args[1];
      if (!sub || sub === 'get') {
        const r = await api.policyGet();
        console.log(JSON.stringify(r, null, 2));
      } else if (sub === 'toggle') {
        const key = args[2];
        if (!key) { console.error('Usage: connector-cli policy toggle <key>'); process.exit(1); }
        const r = await api.policyToggle(key);
        console.log('✅', JSON.stringify(r));
      } else {
        console.error('Usage: connector-cli policy get | connector-cli policy toggle <key>');
      }
      break;
    }

    // ── Skill ─────────────────────────────────────────────────────
    case 'skill': {
      const sub = args[1];
      if (!sub || sub === 'list') {
        const skills = await api.listSkills();
        if (!skills.length) { console.log('(belum ada skill)'); break; }
        for (const s of skills) console.log(`[${s.name}] ${s.description || ''}`);
      } else if (sub === 'add') {
        const name = argVal(args, '--name');
        const desc = argVal(args, '--desc') || argVal(args, '--description') || '';
        const script = argVal(args, '--script');
        const interp = argVal(args, '--interpreter') || 'bash';
        if (!name || !script) { console.error('Usage: connector-cli skill add --name <n> --script "<s>" [--desc <d>] [--interpreter bash]'); process.exit(1); }
        const r = await api.addSkill({ name, description: desc, script, interpreter: interp });
        console.log('✅', JSON.stringify(r));
      }
      break;
    }

    // ── Lint ──────────────────────────────────────────────────────
    case 'lint': {
      const code = args[1];
      if (!code) { console.error('Usage: connector-cli lint "<typescript code>"'); process.exit(1); }
      const violations = await api.lint(code);
      if (!violations.length) { console.log('✅ Tidak ada pelanggaran'); break; }
      for (const v of violations) {
        console.log(`Line ${v.line}: [${v.rule}]`);
        console.log(`  ${v.snippet}`);
        if (v.suggestion) console.log(`  ⮑ ${v.suggestion}`);
      }
      break;
    }

    // ── Setting ───────────────────────────────────────────────────
    case 'setting':
    case 'config': {
      const sub = args[1];
      if (!sub) {
        console.log(JSON.stringify(cfg, null, 2));
      } else if (sub === 'set') {
        const key = args[2];
        const val = args[3];
        if (!key || !val) { console.error('Usage: connector-cli setting set <key> <value>'); process.exit(1); }
        saveConfig({ [key]: val });
        console.log(`✅ ${key} = ${val}`);
      }
      break;
    }

    // ── Cost ─────────────────────────────────────────────────────
    case 'cost': {
      const r = await api.cost();
      console.log(JSON.stringify(r, null, 2));
      break;
    }

    // ── Help ──────────────────────────────────────────────────────
    case 'help':
    case '--help':
    case '-h':
    default: {
      printHelp();
      break;
    }
  }
}

function argVal(args, flag) {
  const idx = args.indexOf(flag);
  if (idx === -1 || idx + 1 >= args.length) return undefined;
  return args[idx + 1];
}

function printHelp() {
  sep();
  console.log(' CONNECTOR-CLI v2 — REST native client (connector-server-rs)');
  sep();
  console.log('');
  console.log(' PERINTAH:');
  console.log('  connector-cli                       Menu TUI interaktif');
  console.log('  connector-cli health                Status server');
  console.log('  connector-cli exec "<cmd>" [opts]   Jalankan perintah di VPS');
  console.log('  connector-cli audit log "<msg>"     Rekam aksi ke audit log');
  console.log('  connector-cli audit records         Lihat riwayat audit');
  console.log('  connector-cli lock <file> [opts]    Kunci file (swarming)');
  console.log('  connector-cli locks                 Lihat semua lock aktif');
  console.log('  connector-cli policy get            Baca policy');
  console.log('  connector-cli policy toggle <key>   Toggle policy');
  console.log('  connector-cli skill list            Daftar skill');
  console.log('  connector-cli skill add [opts]      Tambah skill baru');
  console.log('  connector-cli lint "<ts_code>"      Lint TypeScript code');
  console.log('  connector-cli cost                  Ringkasan token cost');
  console.log('  connector-cli setting               Lihat config');
  console.log('  connector-cli setting set <k> <v>   Set config key');
  console.log('');
  console.log(' OPTIONS UMUM:');
  console.log('  --project <nama>   Nama project (override defaultProject)');
  console.log('  --cwd <dir>        Working directory untuk exec');
  console.log('  --agent <nama>     Filter per agent (untuk audit records)');
  console.log('  --limit <n>        Jumlah record yang ditampilkan');
  console.log('');
  console.log(' CONFIG: ~/.connector-cli/config.json');
  console.log('  connector-cli setting set serverUrl http://IP:3210');
  console.log('  connector-cli setting set agentName alex');
  sep();
}

main().catch((err) => {
  console.error('❌', err.message || err);
  process.exit(1);
});
