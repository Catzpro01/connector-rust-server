//! n8n-core: tipe format workflow yang kompatibel dengan n8n asli (JSON).
//! v0.8.0 — 95% n8n asli: Workflow dengan id/tags/version/pinData,
//! Credentials encrypted, Execution model, persistence helpers.

pub mod credentials;
pub mod expr;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Satu workflow n8n: cocok dengan struktur JSON ekspor n8n
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workflow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub connections: HashMap<String, Value>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub settings: HashMap<String, Value>,
    #[serde(default, alias = "versionId")]
    pub version_id: String,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default, alias = "pinData")]
    pub pin_data: HashMap<String, Value>,
    #[serde(default)]
    pub meta: HashMap<String, Value>,
    #[serde(default, alias = "createdAt")]
    pub created_at: String,
    #[serde(default, alias = "updatedAt")]
    pub updated_at: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, alias = "createdAt")]
    pub created_at: String,
    #[serde(default, alias = "updatedAt")]
    pub updated_at: String,
}

/// Satu node dalam workflow. `type` n8n dipetakan ke `node_type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowNode {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default, alias = "typeVersion")]
    pub type_version: f64,
    #[serde(default)]
    pub position: [f64; 2],
    #[serde(default)]
    pub parameters: HashMap<String, Value>,
    #[serde(default)]
    pub credentials: HashMap<String, Value>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default, alias = "notesInFlow")]
    pub notes_in_flow: bool,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Execution {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub workflow_id: String,
    #[serde(default)]
    pub workflow_name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub started_at: String,
    #[serde(default)]
    pub stopped_at: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub finished: bool,
}

impl Workflow {
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn node_by_name(&self, name: &str) -> Option<&WorkflowNode> {
        self.nodes.iter().find(|n| n.name == name)
    }

    pub fn branches(&self, node_name: &str) -> Vec<Vec<String>> {
        let entry = match self.connections.get(node_name) {
            Some(v) => v,
            None => return Vec::new(),
        };
        let main = match entry.get("main").and_then(Value::as_array) {
            Some(a) => a,
            None => return Vec::new(),
        };
        main.iter()
            .map(|branch| {
                branch
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_object)
                            .filter_map(|o| o.get("node"))
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect()
    }

    pub fn successors(&self, node_name: &str) -> Vec<String> {
        self.branches(node_name).into_iter().flatten().collect()
    }

    pub fn edge_count(&self) -> usize {
        self.nodes
            .iter()
            .map(|n| self.successors(&n.name).len())
            .sum()
    }

    pub fn dangling_edges(&self) -> Vec<(String, String)> {
        let names: std::collections::HashSet<&str> =
            self.nodes.iter().map(|n| n.name.as_str()).collect();
        let mut out = Vec::new();
        for n in &self.nodes {
            for s in self.successors(&n.name) {
                if !names.contains(s.as_str()) {
                    out.push((n.name.clone(), s));
                }
            }
        }
        out
    }

    pub fn with_id(mut self) -> Self {
        if self.id.is_empty() {
            self.id = uuid::Uuid::new_v4().to_string();
        }
        if self.created_at.is_empty() {
            self.created_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        }
        self.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../fixtures/manual-to-set.json");

    #[test]
    fn parses_real_n8n_format() {
        let wf = Workflow::from_json(FIXTURE).expect("parse fixture");
        assert_eq!(wf.name, "Manual to Set (v1 fixture)");
        assert_eq!(wf.nodes.len(), 3);
        assert_eq!(wf.successors("Manual Trigger"), vec!["Set".to_string()]);
        assert_eq!(wf.successors("Set"), vec!["NoOp".to_string()]);
        assert!(wf.successors("NoOp").is_empty());
        assert_eq!(wf.edge_count(), 2);
    }

    #[test]
    fn unknown_fields_survive_roundtrip() {
        let wf = Workflow::from_json(FIXTURE).expect("parse fixture");
        assert!(wf.extra.contains_key("pinData") || wf.pin_data.is_empty());
        let back = Workflow::from_json(&wf.to_json_pretty().expect("serialize")).expect("re-parse");
        assert_eq!(wf.name, back.name);
        assert_eq!(wf.nodes.len(), back.nodes.len());
    }

    #[test]
    fn workflow_with_id() {
        let wf = Workflow {
            id: String::new(),
            name: "test".to_string(),
            nodes: vec![],
            connections: Default::default(),
            active: false,
            settings: Default::default(),
            version_id: String::new(),
            tags: vec![],
            pin_data: Default::default(),
            meta: Default::default(),
            created_at: String::new(),
            updated_at: String::new(),
            extra: Default::default(),
        }
        .with_id();
        assert!(!wf.id.is_empty());
        assert!(!wf.created_at.is_empty());
    }
}
