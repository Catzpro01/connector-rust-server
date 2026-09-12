use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeritageNode {
    pub id: String,
    pub domain: String,
    pub problem: String,
    pub solution: String,
    pub fast_path_cmd: Option<String>,
    pub confidence: f64,
    pub success_count: u32,
    pub failure_count: u32,
    pub tags: Vec<String>,
    pub last_used: u64,
    pub created_at: u64,
}

pub struct HeritageMemoryGraph {
    file_path: PathBuf,
    nodes: HashMap<String, HeritageNode>,
}

impl HeritageMemoryGraph {
    pub fn new(data_dir: &str) -> Self {
        let file_path = PathBuf::from(data_dir).join("heritage_graph.json");
        let mut graph = Self {
            file_path,
            nodes: HashMap::new(),
        };
        graph.load();
        graph
    }

    fn load(&mut self) {
        if self.file_path.exists() {
            if let Ok(raw) = fs::read_to_string(&self.file_path) {
                if let Ok(list) = serde_json::from_str::<Vec<HeritageNode>>(&raw) {
                    for node in list {
                        self.nodes.insert(node.id.clone(), node);
                    }
                }
            }
        }
    }

    fn save(&self) {
        if let Some(parent) = self.file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let list: Vec<&HeritageNode> = self.nodes.values().collect();
        if let Ok(json) = serde_json::to_string_pretty(&list) {
            let _ = fs::write(&self.file_path, json);
        }
    }

    pub fn record_experience(
        &mut self,
        domain: &str,
        problem: &str,
        solution: &str,
        fast_path_cmd: Option<String>,
        tags: Vec<String>,
    ) -> HeritageNode {
        let clean_domain = domain.trim().to_lowercase();
        let clean_problem = problem.trim().to_string();

        for node in self.nodes.values_mut() {
            if node.domain == clean_domain && node.problem.eq_ignore_ascii_case(&clean_problem) {
                node.solution = solution.to_string();
                if fast_path_cmd.is_some() {
                    node.fast_path_cmd = fast_path_cmd;
                }
                node.success_count += 1;
                node.confidence = (node.success_count as f64 / (node.success_count + node.failure_count) as f64).min(1.0);
                node.last_used = chrono::Utc::now().timestamp() as u64;
                let updated = node.clone();
                self.save();
                return updated;
            }
        }

        let now = chrono::Utc::now().timestamp() as u64;
        let id = format!("herit-{}", now);
        let node = HeritageNode {
            id: id.clone(),
            domain: clean_domain,
            problem: clean_problem,
            solution: solution.trim().to_string(),
            fast_path_cmd,
            confidence: 0.65,
            success_count: 1,
            failure_count: 0,
            tags,
            last_used: now,
            created_at: now,
        };

        self.nodes.insert(id, node.clone());
        self.save();
        node
    }

    pub fn query_fast_path(&self, query: &str) -> Option<HeritageNode> {
        let q = query.to_lowercase();
        let mut best: Option<HeritageNode> = None;
        let mut highest_score = 0.0;

        for node in self.nodes.values() {
            let mut score = 0.0;
            if node.problem.to_lowercase().contains(&q) || q.contains(&node.problem.to_lowercase()) {
                score += 3.0;
            }
            for tag in &node.tags {
                if q.contains(tag) {
                    score += 1.0;
                }
            }

            let total = score * node.confidence;
            if total > highest_score && total >= 1.0 {
                highest_score = total;
                best = Some(node.clone());
            }
        }

        best
    }

    pub fn list_all(&self) -> Vec<HeritageNode> {
        let mut list: Vec<HeritageNode> = self.nodes.values().cloned().collect();
        list.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        list
    }
}
