use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsageRecord {
    pub id: String,
    pub agent: String,
    pub project: String,
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub cost_idr: f64,
    pub timestamp: u64,
    pub timestamp_wib: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub total_tokens_formatted: String,
    pub cost_usd: f64,
    pub cost_usd_formatted: String,
    pub cost_idr: f64,
    pub cost_idr_formatted: String,
    pub interaction_count: usize,
}

const DEFAULT_USD_TO_IDR: f64 = 16250.0;

fn format_tokens(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.2}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

fn format_usd(amount: f64) -> String {
    if amount < 0.001 && amount > 0.0 {
        "< $0.001".to_string()
    } else {
        format!("${:.3}", amount)
    }
}

fn format_idr(amount: f64) -> String {
    format!("Rp {:.0}", amount)
}

pub struct TokenCostTracker {
    file_path: PathBuf,
    records: Vec<TokenUsageRecord>,
}

impl TokenCostTracker {
    pub fn new(data_dir: &str) -> Self {
        let file_path = PathBuf::from(data_dir).join("token_costs.json");
        let mut tracker = Self {
            file_path,
            records: Vec::new(),
        };
        tracker.load();
        tracker
    }

    fn load(&mut self) {
        if self.file_path.exists() {
            if let Ok(raw) = fs::read_to_string(&self.file_path) {
                if let Ok(records) = serde_json::from_str::<Vec<TokenUsageRecord>>(&raw) {
                    self.records = records;
                }
            }
        }
    }

    fn save(&self) {
        if let Some(parent) = self.file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.records) {
            let _ = fs::write(&self.file_path, json);
        }
    }

    pub fn record_usage(
        &mut self,
        agent: &str,
        project: &str,
        prompt_tokens: u64,
        completion_tokens: u64,
        model: &str,
    ) -> TokenUsageRecord {
        let clean_agent = agent.trim().to_lowercase();
        let clean_project = project.trim().to_lowercase();

        let prompt_rate = 3.0; // $3/1M
        let comp_rate = 15.0;  // $15/1M

        let cost_usd = (prompt_tokens as f64 / 1_000_000.0) * prompt_rate
            + (completion_tokens as f64 / 1_000_000.0) * comp_rate;
        let cost_idr = cost_usd * DEFAULT_USD_TO_IDR;
        let total_tokens = prompt_tokens + completion_tokens;

        let now = chrono::Utc::now();
        let timestamp_wib = now.format("%d-%m-%Y %H:%M:%S WIB").to_string();

        let rec = TokenUsageRecord {
            id: format!("t-{}", now.timestamp_millis()),
            agent: clean_agent,
            project: clean_project,
            model: model.to_string(),
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cost_usd,
            cost_idr,
            timestamp: now.timestamp() as u64,
            timestamp_wib,
        };

        self.records.push(rec.clone());
        if self.records.len() > 5000 {
            self.records.drain(0..(self.records.len() - 5000));
        }
        self.save();
        rec
    }

    pub fn estimate_and_record(
        &mut self,
        agent: &str,
        project: &str,
        prompt_text: &str,
        output_text: &str,
    ) -> TokenUsageRecord {
        let prompt_tokens = (prompt_text.len() as f64 / 3.8).max(1.0).round() as u64;
        let completion_tokens = if !output_text.is_empty() {
            (output_text.len() as f64 / 3.8).max(1.0).round() as u64
        } else {
            0
        };
        self.record_usage(agent, project, prompt_tokens, completion_tokens, "default")
    }

    pub fn get_global_summary(&self) -> CostSummary {
        self.aggregate(&self.records)
    }

    pub fn get_cost_by_agent(&self, agent: &str) -> CostSummary {
        let clean = agent.trim().to_lowercase();
        let filtered: Vec<TokenUsageRecord> = self
            .records
            .iter()
            .filter(|r| r.agent == clean)
            .cloned()
            .collect();
        self.aggregate(&filtered)
    }

    pub fn get_cost_by_project(&self, project: &str) -> CostSummary {
        let clean = project.trim().to_lowercase();
        let filtered: Vec<TokenUsageRecord> = self
            .records
            .iter()
            .filter(|r| r.project == clean)
            .cloned()
            .collect();
        self.aggregate(&filtered)
    }

    fn aggregate(&self, recs: &[TokenUsageRecord]) -> CostSummary {
        let mut prompt_tokens = 0;
        let mut completion_tokens = 0;
        let mut cost_usd = 0.0;
        let mut cost_idr = 0.0;

        for r in recs {
            prompt_tokens += r.prompt_tokens;
            completion_tokens += r.completion_tokens;
            cost_usd += r.cost_usd;
            cost_idr += r.cost_idr;
        }

        let total_tokens = prompt_tokens + completion_tokens;

        CostSummary {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            total_tokens_formatted: format_tokens(total_tokens),
            cost_usd,
            cost_usd_formatted: format_usd(cost_usd),
            cost_idr,
            cost_idr_formatted: format_idr(cost_idr),
            interaction_count: recs.len(),
        }
    }
}
