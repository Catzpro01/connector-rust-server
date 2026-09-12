use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPolicy {
    pub stealth_trap: bool,
    pub auto_import_memory: bool,
    pub auto_git_sync: bool,
    pub syncthing_sync: bool,
    pub terminal_locked: bool,
}

impl Default for AgentPolicy {
    fn default() -> Self {
        Self {
            stealth_trap: true,
            auto_import_memory: true,
            auto_git_sync: true,
            syncthing_sync: false,
            terminal_locked: false,
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

    pub fn toggle_stealth_trap(&mut self) -> bool {
        self.policy.stealth_trap = !self.policy.stealth_trap;
        self.save();
        self.policy.stealth_trap
    }

    pub fn toggle_auto_import_memory(&mut self) -> bool {
        self.policy.auto_import_memory = !self.policy.auto_import_memory;
        self.save();
        self.policy.auto_import_memory
    }

    pub fn toggle_auto_git_sync(&mut self) -> bool {
        self.policy.auto_git_sync = !self.policy.auto_git_sync;
        self.save();
        self.policy.auto_git_sync
    }

    pub fn toggle_syncthing(&mut self) -> bool {
        self.policy.syncthing_sync = !self.policy.syncthing_sync;
        self.save();
        self.policy.syncthing_sync
    }

    pub fn toggle_terminal_lock(&mut self) -> bool {
        self.policy.terminal_locked = !self.policy.terminal_locked;
        self.save();
        self.policy.terminal_locked
    }
}
