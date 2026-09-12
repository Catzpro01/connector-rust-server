use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub id: String,
    pub timestamp_wib: String,
    pub agent: String,
    pub project: String,
    pub prompt: String,
    pub output_snippet: Option<String>,
}

pub struct AuditManager {
    file_path: String,
    records: Vec<AuditRecord>,
}

impl AuditManager {
    pub fn new(data_dir: &str) -> Self {
        let file_path = format!("{}/audit_records.json", data_dir);
        let mut records = Vec::new();

        if Path::new(&file_path).exists() {
            if let Ok(content) = fs::read_to_string(&file_path) {
                if let Ok(list) = serde_json::from_str::<Vec<AuditRecord>>(&content) {
                    records = list;
                }
            }
        }

        Self { file_path, records }
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.records) {
            let _ = fs::write(&self.file_path, json);
        }
    }

    pub fn record(&mut self, agent: &str, project: &str, prompt: &str, output: Option<&str>) {
        let now: DateTime<Utc> = Utc::now();
        let wib_time = now + chrono::Duration::hours(7);
        let timestamp_wib = wib_time.format("%d/%m %H:%M:%S WIB").to_string();

        let record = AuditRecord {
            id: format!("rec-{}", now.timestamp_millis()),
            timestamp_wib,
            agent: agent.to_string(),
            project: project.to_string(),
            prompt: prompt.to_string(),
            output_snippet: output.map(|o| {
                if o.len() > 200 {
                    format!("{}...", &o[..200])
                } else {
                    o.to_string()
                }
            }),
        };

        self.records.push(record);
        if self.records.len() > 10000 {
            self.records.remove(0);
        }
        self.save();
    }

    pub fn get_agent_records(&self, agent: &str, limit: usize) -> Vec<AuditRecord> {
        let target = agent.to_lowercase();
        self.records
            .iter()
            .rev()
            .filter(|r| r.agent.to_lowercase() == target)
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn get_agent_prompt_count(&self, agent: &str) -> usize {
        let target = agent.to_lowercase();
        self.records
            .iter()
            .filter(|r| r.agent.to_lowercase() == target)
            .count()
    }

    pub fn get_recent_records(&self, limit: usize) -> Vec<AuditRecord> {
        self.records.iter().rev().take(limit).cloned().collect()
    }

    pub fn clear_records(&mut self) {
        self.records.clear();
        self.save();
    }
}
