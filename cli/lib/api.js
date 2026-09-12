/ lib/api.js — REST client untuk connector-server-rs
// Semua 12 endpoint didukung, zero external dependency (pakai native fetch Node 18+)

export class ConnectorAPI {
  /**
   * @param {{ serverUrl: string, agentName: string, defaultProject?: string }} config
   */
  constructor(config) {
    this.base = config.serverUrl.replace(/\/$/, '');
    this.agent = config.agentName;
    this.project = config.defaultProject || '';
  }

  async _fetch(method, path, body) {
    const opts = {
      method,
      headers: { 'Content-Type': 'application/json', 'X-Agent-Name': this.agent },
    };
    if (body !== undefined) opts.body = JSON.stringify(body);
    const res = await fetch(`${this.base}${path}`, opts);
    const text = await res.text();
    let json;
    try { json = JSON.parse(text); } catch { json = { raw: text }; }
    if (!res.ok) throw Object.assign(new Error(`HTTP ${res.status}: ${text}`), { status: res.status, body: json });
    return json;
  }

  // ── Health & Cost ──────────────────────────────────────────────
  health()  { return this._fetch('GET', '/health'); }
  cost()    { return this._fetch('GET', '/api/cost'); }

  // ── Policy ────────────────────────────────────────────────────
  policyGet()             { return this._fetch('GET', '/api/policy'); }
  policyToggle(toggle)    { return this._fetch('POST', '/api/policy', { toggle }); }

  // ── Shell Exec ────────────────────────────────────────────────
  /**
   * @param {string} command
   * @param {{ project?: string, cwd?: string }} opts
   */
  shellExec(command, opts = {}) {
    return this._fetch('POST', '/api/shell/exec', {
      command,
      agent: this.agent,
      project: opts.project || this.project || undefined,
      cwd: opts.cwd || undefined,
    });
  }

  // ── Audit ─────────────────────────────────────────────────────
  auditLog(prompt, output, project) {
    return this._fetch('POST', '/api/audit/log', {
      agent: this.agent,
      project: project || this.project || undefined,
      prompt,
      output: output || undefined,
    });
  }
  /** @param {{ agent?: string, limit?: number }} opts */
  auditRecords(opts = {}) {
    const q = new URLSearchParams();
    if (opts.agent) q.set('agent', opts.agent);
    if (opts.limit) q.set('limit', String(opts.limit));
    const qs = q.toString() ? `?${q}` : '';
    return this._fetch('GET', `/api/audit/records${qs}`);
  }

  // ── Swarming ──────────────────────────────────────────────────
  /** @param {{ project: string, file_path: string, lease_minutes?: number }} opts */
  lockFile(opts) {
    return this._fetch('POST', '/api/swarming/lock', {
      project: opts.project || this.project,
      file_path: opts.file_path,
      agent: this.agent,
      lease_minutes: opts.lease_minutes || 30,
    });
  }
  listLocks() { return this._fetch('GET', '/api/swarming/locks'); }

  // ── Evolution ─────────────────────────────────────────────────
  listSkills() { return this._fetch('GET', '/api/evolution/skills'); }
  /** @param {{ name: string, description: string, script: string, interpreter?: string }} opts */
  addSkill(opts) {
    return this._fetch('POST', '/api/evolution/synthesize', {
      name: opts.name,
      description: opts.description,
      script: opts.script,
      interpreter: opts.interpreter || 'bash',
    });
  }

  // ── Lint ──────────────────────────────────────────────────────
  lint(code) { return this._fetch('POST', '/api/lint/mattpocock', { code }); }
}
