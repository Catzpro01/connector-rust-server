use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPolicy {
    #[serde(rename = "sessionPersistence")]
    pub session_persistence: bool,
    #[serde(rename = "cacheWarmup")]
    pub cache_warmup: bool,
    #[serde(rename = "backgroundBackup")]
    pub background_backup: bool,
    #[serde(rename = "peerSync")]
    pub peer_sync: bool,
    #[serde(rename = "maintenanceMode")]
    pub maintenance_mode: bool,
}

impl Default for AgentPolicy {
    fn default() -> Self {
        Self {
            session_persistence: true,
            cache_warmup: true,
            background_backup: true,
            peer_sync: false,
            maintenance_mode: false,
        }
    }
}

pub struct PolicyManager {
    file_path: String,
    pub policy: AgentPolicy,
}

impl PolicyManager {
    pub fn new(data_dir: &str) -> Self {
        let file_path = format!("{}/agent_policy.json", data_dir);
        let mut policy = AgentPolicy::default();

        if Path::new(&file_path).exists() {
            if let Ok(content) = fs::read_to_string(&file_path) {
                if let Ok(saved) = serde_json::from_str::<AgentPolicy>(&content) {
                    policy = saved;
                }
            }
        } else {
            let _ = fs::create_dir_all(data_dir);
            if let Ok(json) = serde_json::to_string_pretty(&policy) {
                let _ = fs::write(&file_path, json);
            }
        }

        Self { file_path, policy }
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.policy) {
            let _ = fs::write(&self.file_path, json);
        }
    }

    pub fn toggle_session_persistence(&mut self) -> bool {
        self.policy.session_persistence = !self.policy.session_persistence;
        self.save();
        self.policy.session_persistence
    }

    pub fn toggle_cache_warmup(&mut self) -> bool {
        self.policy.cache_warmup = !self.policy.cache_warmup;
        self.save();
        self.policy.cache_warmup
    }

    pub fn toggle_background_backup(&mut self) -> bool {
        self.policy.background_backup = !self.policy.background_backup;
        self.save();
        self.policy.background_backup
    }

    pub fn toggle_peer_sync(&mut self) -> bool {
        self.policy.peer_sync = !self.policy.peer_sync;
        self.save();
        self.policy.peer_sync
    }

    pub fn toggle_maintenance_mode(&mut self) -> bool {
        self.policy.maintenance_mode = !self.policy.maintenance_mode;
        self.save();
        self.policy.maintenance_mode
    }
}
