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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRate {
    pub name: String,
    pub input_rate_per_m: f64,
    pub output_rate_per_m: f64,
}

pub struct TokenCostTracker {
    file_path: PathBuf,
    rates_file_path: PathBuf,
    records: Vec<TokenUsageRecord>,
    model_rates: std::collections::HashMap<String, ModelRate>,
    active_model: String,
}

impl TokenCostTracker {
    pub fn new(data_dir: &str) -> Self {
        let file_path = PathBuf::from(data_dir).join("token_costs.json");
        let rates_file_path = PathBuf::from(data_dir).join("model_rates.json");
        let mut tracker = Self {
            file_path,
            rates_file_path,
            records: Vec::new(),
            model_rates: std::collections::HashMap::new(),
            active_model: "claude-3-7-sonnet".to_string(),
        };
        tracker.init_default_rates();
        tracker.load_rates();
        tracker.load();
        tracker
    }

    fn init_default_rates(&mut self) {
        let defaults = vec![
            ModelRate { name: "claude-3-7-sonnet".into(), input_rate_per_m: 3.0, output_rate_per_m: 15.0 },
            ModelRate { name: "gpt-4o".into(), input_rate_per_m: 2.5, output_rate_per_m: 10.0 },
            ModelRate { name: "gemini-1.5-pro".into(), input_rate_per_m: 1.25, output_rate_per_m: 5.0 },
            ModelRate { name: "deepseek-v3".into(), input_rate_per_m: 0.27, output_rate_per_m: 1.10 },
            ModelRate { name: "default".into(), input_rate_per_m: 3.0, output_rate_per_m: 15.0 },
        ];
        for d in defaults {
            self.model_rates.insert(d.name.clone(), d);
        }
    }

    fn load_rates(&mut self) {
        if self.rates_file_path.exists() {
            if let Ok(raw) = fs::read_to_string(&self.rates_file_path) {
                if let Ok(list) = serde_json::from_str::<Vec<ModelRate>>(&raw) {
                    for r in list {
                        self.model_rates.insert(r.name.clone(), r);
                    }
                }
            }
        }
    }

    fn save_rates(&self) {
        if let Some(parent) = self.rates_file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let list: Vec<&ModelRate> = self.model_rates.values().collect();
        if let Ok(json) = serde_json::to_string_pretty(&list) {
            let _ = fs::write(&self.rates_file_path, json);
        }
    }

    pub fn set_model_rate(&mut self, model: &str, input_rate: f64, output_rate: f64) {
        let clean = model.trim().to_lowercase();
        self.model_rates.insert(
            clean.clone(),
            ModelRate {
                name: clean,
                input_rate_per_m: input_rate,
                output_rate_per_m: output_rate,
            },
        );
        self.save_rates();
    }

    pub fn set_active_model(&mut self, model: &str) {
        self.active_model = model.trim().to_lowercase();
    }

    pub fn get_active_model(&self) -> String {
        self.active_model.clone()
    }

    pub fn list_models(&self) -> Vec<ModelRate> {
        self.model_rates.values().cloned().collect()
    }

    pub fn reset_agent_cost(&mut self, agent: &str) {
        let clean = agent.trim().to_lowercase();
        self.records.retain(|r| r.agent != clean);
        self.save();
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
        let clean_model = if model.is_empty() {
            self.active_model.clone()
        } else {
            model.trim().to_lowercase()
        };

        let rate = self
            .model_rates
            .get(&clean_model)
            .or_else(|| self.model_rates.get(&self.active_model))
            .or_else(|| self.model_rates.get("default"));

        let (prompt_rate, comp_rate) = match rate {
            Some(r) => (r.input_rate_per_m, r.output_rate_per_m),
            None => (3.0, 15.0),
        };

        let cost_usd = (prompt_tokens as f64 / 1_000_000.0) * prompt_rate
            + (completion_tokens as f64 / 1_000_000.0) * comp_rate;
        let cost_idr = cost_usd * DEFAULT_USD_TO_IDR;
        let total_tokens = prompt_tokens + completion_tokens;

        let now = chrono::Utc::now();
        let timestamp_wib = (now + chrono::Duration::hours(7)).format("%d/%m %H:%M:%S WIB").to_string();

        let rec = TokenUsageRecord {
            id: format!("t-{}", now.timestamp_millis()),
            agent: clean_agent,
            project: clean_project,
            model: clean_model,
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
        // Pure agent input tokens (what agent typed/prompted)
        let prompt_tokens = (prompt_text.len() as f64 / 3.8).max(1.0).round() as u64;
        // Pure agent output tokens (agent reasoning/reply, capped to avoid terminal stdout pollution)
        let completion_tokens = if !output_text.is_empty() {
            let capped_len = output_text.len().min(1000);
            (capped_len as f64 / 3.8).max(1.0).round() as u64
        } else {
            0
        };
        let active = self.active_model.clone();
        self.record_usage(agent, project, prompt_tokens, completion_tokens, &active)
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
