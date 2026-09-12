// lib/config.js -- Config manager untuk connector-cli
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import { createInterface } from 'readline';

const CONFIG_DIR_DEFAULT = join(homedir(), '.connector-cli');

/** @returns {{ serverUrl: string, agentName: string, defaultProject: string }} */
export function loadConfig(configDir = CONFIG_DIR_DEFAULT) {
  const configFile = join(configDir, 'config.json');
  if (!existsSync(configFile)) {
    return { serverUrl: 'http://103.55.37.234:3210', agentName: '', defaultProject: '' };
  }
  try {
    return JSON.parse(readFileSync(configFile, 'utf8'));
  } catch {
    return { serverUrl: 'http://103.55.37.234:3210', agentName: '', defaultProject: '' };
  }
}

/** @param {{ serverUrl?: string, agentName?: string, defaultProject?: string }} updates */
export function saveConfig(updates, configDir = CONFIG_DIR_DEFAULT) {
  const configFile = join(configDir, 'config.json');
  mkdirSync(configDir, { recursive: true });
  const existing = loadConfig(configDir);
  writeFileSync(configFile, JSON.stringify({ ...existing, ...updates }, null, 2), 'utf8');
}

/** @param {string} question @returns {Promise<string>} */
export function prompt(question) {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  return new Promise((resolve) => { rl.question(question, (ans) => { rl.close(); resolve(ans.trim()); }); });
}

/** Pastikan agentName terisi, jika belum tanya ke user */
export async function ensureAgentName(configDir = CONFIG_DIR_DEFAULT) {
  const cfg = loadConfig(configDir);
  if (cfg.agentName) return cfg.agentName;
  const name = await prompt('Login agent (nama tanpa password): ');
  if (!name) throw new Error('Agent name wajib diisi');
  saveConfig({ agentName: name }, configDir);
  return name;
}
