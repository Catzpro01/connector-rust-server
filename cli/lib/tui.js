/ lib/tui.js — Interactive TUI menu untuk connector-cli
import { createInterface } from 'readline';
import { prompt } from './config.js';

/** Print separator line */
export function sep() { console.log('━'.repeat(55)); }

/** @param {string} title @param {string} agent @param {string} serverUrl */
export function printHeader(title, agent, serverUrl) {
  sep();
  console.log(` CONNECTOR-CLI v2  |  ${agent}  |  ${serverUrl}`);
  sep();
  console.log(` ${title}`);
  sep();
}

/** @param {string} label @returns {string} */
function statusBadge(val) {
  return val ? '✅ ON ' : '⬜ OFF';
}

/**
 * Tampilkan menu interaktif lengkap
 * @param {import('./api.js').ConnectorAPI} api
 * @param {{ serverUrl: string, agentName: string, defaultProject: string }} cfg
 * @param {(updates: object) => void} saveConfig
 */
export async function interactiveMenu(api, cfg, saveConfig) {
  while (true) {
    console.clear();
    printHeader('MENU UTAMA', cfg.agentName, cfg.serverUrl);
    console.log('  1. status server       — health + policy');
    console.log('  2. jalankan perintah   — shell/exec di VPS');
    console.log('  3. lihat audit log     — rekaman aksi agent');
    console.log('  4. lock file           — kunci file (swarming)');
    console.log('  5. lihat semua lock    — daftar lock aktif');
    console.log('  6. skill               — lihat / tambah skill');
    console.log('  7. toggle policy       — stealth_trap, terminal_locked, dsb');
    console.log('  8. setting             — ubah server URL, agent name');
    console.log('  9. exit');
    sep();

    const choice = await prompt('Pilih (1-9): ');

    switch (choice) {
      case '1': await menuStatus(api); break;
      case '2': await menuExec(api, cfg); break;
      case '3': await menuAudit(api, cfg); break;
      case '4': await menuLock(api, cfg); break;
      case '5': await menuLocks(api); break;
      case '6': await menuSkill(api); break;
      case '7': await menuPolicy(api); break;
      case '8': cfg = await menuSetting(cfg, saveConfig); break;
      case '9': console.log('Bye.'); process.exit(0); break;
      default: console.log('Pilihan tidak valid.'); await pause();
    }
  }
}

async function menuStatus(api) {
  console.log('\n⏳ Mengambil status...');
  try {
    const health = await api.health();
    const policy = await api.policyGet();
    const cost = await api.cost();
    sep();
    console.log('🟢 SERVER ONLINE');
    console.log(`   engine  : ${health.engine}`);
    console.log(`   version : ${health.version}`);
    console.log(`   skills  : ${health.active_skills}`);
    console.log(`   RAM     : ${health.memory_mb} MB`);
    sep();
    console.log('📋 POLICY');
    console.log(`   stealth_trap      : ${statusBadge(policy.stealth_trap)}`);
    console.log(`   terminal_locked   : ${statusBadge(policy.terminal_locked)}`);
    console.log(`   auto_import_memory: ${statusBadge(policy.auto_import_memory)}`);
    console.log(`   auto_git_sync     : ${statusBadge(policy.auto_git_sync)}`);
    console.log(`   syncthing_sync    : ${statusBadge(policy.syncthing_sync)}`);
    sep();
    console.log('💰 TOKEN COST');
    console.log(`   total input  : ${cost.total_input_tokens ?? 0}`);
    console.log(`   total output : ${cost.total_output_tokens ?? 0}`);
    console.log(`   est. cost    : $${(cost.total_cost_usd ?? 0).toFixed(4)}`);
  } catch (e) {
    console.error('❌ Error:', e.message);
  }
  await pause();
}

async function menuExec(api, cfg) {
  const cmd = await prompt('Perintah yang akan dijalankan: ');
  if (!cmd) return;
  const proj = await prompt(`Project (Enter = "${cfg.defaultProject}"): `);
  const cwd = await prompt('Working dir (Enter = default): ');
  console.log('\n⏳ Menjalankan...');
  try {
    const r = await api.shellExec(cmd, {
      project: proj || cfg.defaultProject || undefined,
      cwd: cwd || undefined,
    });
    sep();
    if (r.success) {
      console.log('✅ Sukses (exit:', r.exit_code, ')');
      if (r.stdout) console.log('\nSTDOUT:\n' + r.stdout.slice(0, 2000));
      if (r.stderr) console.log('\nSTDERR:\n' + r.stderr.slice(0, 500));
    } else {
      console.log('❌ Gagal (exit:', r.exit_code, ')');
      if (r.stderr) console.log('\nSTDERR:\n' + r.stderr);
      if (r.error) console.log('\nERROR:', r.error);
    }
  } catch (e) {
    console.error('❌ Error:', e.message);
  }
  await pause();
}

async function menuAudit(api, cfg) {
  const filterAgent = await prompt(`Filter agent (Enter = semua, default "${cfg.agentName}"): `);
  const limitStr = await prompt('Limit (Enter = 10): ');
  console.log('\n⏳ Mengambil...');
  try {
    const records = await api.auditRecords({
      agent: filterAgent || undefined,
      limit: parseInt(limitStr || '10') || 10,
    });
    sep();
    if (!records.length) { console.log('(belum ada rekaman)'); }
    for (const r of records) {
      console.log(`\n[${r.timestamp_wib}] ${r.agent} / ${r.project || '-'}`);
      console.log(`  PROMPT: ${(r.prompt || '').slice(0, 120)}`);
      if (r.output_snippet) console.log(`  OUTPUT: ${r.output_snippet.slice(0, 100)}`);
    }
  } catch (e) {
    console.error('❌ Error:', e.message);
  }
  await pause();
}

async function menuLock(api, cfg) {
  const filePath = await prompt('Path file yang akan dikunci: ');
  if (!filePath) return;
  const proj = await prompt(`Project (Enter = "${cfg.defaultProject}"): `);
  const minStr = await prompt('Durasi lock dalam menit (Enter = 30): ');
  console.log('\n⏳ Mengunci...');
  try {
    const r = await api.lockFile({
      file_path: filePath,
      project: proj || cfg.defaultProject || 'default',
      lease_minutes: parseInt(minStr || '30') || 30,
    });
    sep();
    if (r.success) console.log(`✅ File "${filePath}" berhasil dikunci oleh ${r.agent}`);
    else console.log(`❌ Gagal: file sudah dikunci agent lain`);
  } catch (e) {
    console.error('❌ Error:', e.message);
  }
  await pause();
}

async function menuLocks(api) {
  console.log('\n⏳ Mengambil lock aktif...');
  try {
    const locks = await api.listLocks();
    sep();
    if (!locks.length) { console.log('(tidak ada lock aktif)'); }
    for (const l of locks) {
      console.log(`  ${l.file_path}  [${l.project}]  — agent: ${l.agent}  exp: ${l.expires_at || '-'}`);
    }
  } catch (e) {
    console.error('❌ Error:', e.message);
  }
  await pause();
}

async function menuSkill(api) {
  console.log('\n  a. lihat semua skill');
  console.log('  b. tambah skill baru');
  const c = await prompt('Pilih (a/b): ');
  if (c === 'a') {
    console.log('\n⏳ Mengambil skill...');
    try {
      const skills = await api.listSkills();
      sep();
      if (!skills.length) { console.log('(belum ada skill)'); }
      for (const s of skills) {
        console.log(`  [${s.name}] ${s.description || ''}`);
      }
    } catch (e) {
      console.error('❌ Error:', e.message);
    }
  } else if (c === 'b') {
    const name = await prompt('Nama skill: ');
    const desc = await prompt('Deskripsi: ');
    const script = await prompt('Script (satu baris): ');
    const interp = await prompt('Interpreter (Enter = bash): ');
    console.log('\n⏳ Mendaftarkan...');
    try {
      const r = await api.addSkill({ name, description: desc, script, interpreter: interp || 'bash' });
      sep();
      console.log('✅ Skill terdaftar:', JSON.stringify(r));
    } catch (e) {
      console.error('❌ Error:', e.message);
    }
  }
  await pause();
}

async function menuPolicy(api) {
  const choices = ['stealth_trap', 'auto_import_memory', 'auto_git_sync', 'syncthing_sync', 'terminal_locked'];
  console.log('\nPolicy yang tersedia:');
  choices.forEach((k, i) => console.log(`  ${i+1}. ${k}`));
  const c = await prompt('Toggle yang mana? (1-5): ');
  const key = choices[parseInt(c) - 1];
  if (!key) { console.log('Pilihan tidak valid.'); await pause(); return; }
  console.log(`\n⏳ Toggle ${key}...`);
  try {
    const r = await api.policyToggle(key);
    sep();
    console.log('✅ Policy sekarang:', JSON.stringify(r));
  } catch (e) {
    console.error('❌ Error:', e.message);
  }
  await pause();
}

async function menuSetting(cfg, saveConfig) {
  console.log('\nSetting saat ini:');
  console.log(`  serverUrl     : ${cfg.serverUrl}`);
  console.log(`  agentName     : ${cfg.agentName}`);
  console.log(`  defaultProject: ${cfg.defaultProject}`);
  sep();
  const url = await prompt(`Server URL baru (Enter = tidak berubah): `);
  const name = await prompt(`Nama agent baru (Enter = tidak berubah): `);
  const proj = await prompt(`Default project baru (Enter = tidak berubah): `);
  const updated = {
    serverUrl: url || cfg.serverUrl,
    agentName: name || cfg.agentName,
    defaultProject: proj || cfg.defaultProject,
  };
  saveConfig(updated);
  console.log('✅ Setting tersimpan.');
  await pause();
  return updated;
}

function pause() {
  return new Promise((resolve) => {
    const rl = createInterface({ input: process.stdin, output: process.stdout });
    rl.question('\n[Enter untuk lanjut]', () => { rl.close(); resolve(); });
  });
}
