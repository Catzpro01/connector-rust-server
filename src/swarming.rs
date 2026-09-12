use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLock {
    pub file_path: String,
    pub project: String,
    pub agent: String,
    pub acquired_at: u64,
    pub lease_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHandoff {
    pub id: String,
    pub project: String,
    pub from_agent: String,
    pub to_agent: Option<String>,
    pub scope: String,
    pub summary: String,
    pub timestamp: u64,
}

pub struct ZeroCollisionSwarming {
    locks: HashMap<String, FileLock>,
    handoffs: Vec<AgentHandoff>,
}

impl ZeroCollisionSwarming {
    pub fn new() -> Self {
        Self {
            locks: HashMap::new(),
            handoffs: Vec::new(),
        }
    }

    pub fn acquire_lock(
        &mut self,
        project: &str,
        file_path: &str,
        agent: &str,
        lease_minutes: u64,
    ) -> Result<String, String> {
        let key = format!("{}:{}", project, file_path);
        let now = chrono::Utc::now().timestamp() as u64;

        if let Some(existing) = self.locks.get(&key) {
            if now.saturating_sub(existing.acquired_at) < (existing.lease_ms / 1000) {
                if existing.agent.eq_ignore_ascii_case(agent) {
                    return Ok("Lock diperpanjang.".to_string());
                }
                return Err(format!(
                    "File '{}' sedang dikerjakan oleh agent '{}'. Hindari tabrakan!",
                    file_path, existing.agent
                ));
            }
        }

        let lock = FileLock {
            file_path: file_path.to_string(),
            project: project.to_string(),
            agent: agent.to_string(),
            acquired_at: now,
            lease_ms: lease_minutes * 60 * 1000,
        };

        self.locks.insert(key, lock);
        Ok(format!("Write lock berhasil diberikan ke agent '{}'.", agent))
    }

    pub fn release_lock(&mut self, project: &str, file_path: &str, agent: &str) -> bool {
        let key = format!("{}:{}", project, file_path);
        if let Some(existing) = self.locks.get(&key) {
            if existing.agent.eq_ignore_ascii_case(agent) {
                self.locks.remove(&key);
                return true;
            }
        }
        false
    }

    pub fn record_handoff(
        &mut self,
        project: &str,
        from_agent: &str,
        to_agent: Option<String>,
        scope: &str,
        summary: &str,
    ) -> AgentHandoff {
        let now = chrono::Utc::now().timestamp() as u64;
        let handoff = AgentHandoff {
            id: format!("ho-{}", now),
            project: project.to_string(),
            from_agent: from_agent.to_string(),
            to_agent,
            scope: scope.to_string(),
            summary: summary.to_string(),
            timestamp: now,
        };

        self.handoffs.push(handoff.clone());
        if self.handoffs.len() > 200 {
            self.handoffs.drain(0..(self.handoffs.len() - 200));
        }
        handoff
    }

    pub fn list_active_locks(&self, project: Option<&str>) -> Vec<FileLock> {
        let now = chrono::Utc::now().timestamp() as u64;
        self.locks
            .values()
            .filter(|l| {
                let valid = now.saturating_sub(l.acquired_at) < (l.lease_ms / 1000);
                if let Some(p) = project {
                    valid && l.project == p
                } else {
                    valid
                }
            })
            .cloned()
            .collect()
    }
}
