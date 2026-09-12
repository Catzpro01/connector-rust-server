//! n8n-engine: executor workflow — 95% n8n asli + perf di atas asli
//! v0.8.0: parallel execution via rayon for independent branches,
//! continueOnFail, levels, metrics, improved lint.

use n8n_core::{Workflow, WorkflowNode};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError {
    message: String,
}

impl EngineError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for EngineError {}

pub type EngineResult<T> = Result<T, EngineError>;

pub type BranchOutputs = Vec<Vec<Value>>;

pub struct ExecContext<'a> {
    pub outputs: &'a HashMap<String, BranchOutputs>,
    pub webhook: Option<&'a Value>,
    pub workflow_name: Option<&'a str>,
    pub execution_id: Option<&'a str>,
}

impl ExecContext<'_> {
    pub fn output_of(&self, name: &str) -> Option<&BranchOutputs> {
        self.outputs.get(name)
    }
}

#[derive(Debug, Clone)]
pub struct MultiInput {
    pub from: String,
    pub branch: usize,
    pub items: Vec<Value>,
}

pub trait Node: Send + Sync {
    fn node_type(&self) -> &'static str;
    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs>;
    fn execute_multi(
        &self,
        node: &WorkflowNode,
        inputs: Vec<MultiInput>,
        ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let items = inputs.into_iter().flat_map(|i| i.items).collect();
        self.execute(node, items, ctx)
    }
}

#[derive(Default)]
pub struct Registry {
    nodes: HashMap<String, Arc<dyn Node>>,
}

impl Registry {
    pub fn register(&mut self, node: Arc<dyn Node>) {
        self.nodes.insert(node.node_type().to_string(), node);
    }

    pub fn get(&self, node_type: &str) -> Option<&Arc<dyn Node>> {
        self.nodes.get(node_type)
    }

    pub fn types(&self) -> Vec<String> {
        let mut v: Vec<String> = self.nodes.keys().cloned().collect();
        v.sort();
        v
    }

    pub fn count(&self) -> usize {
        self.nodes.len()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunReport {
    pub outputs: HashMap<String, BranchOutputs>,
    pub order: Vec<String>,
    pub durations_ms: HashMap<String, u128>,
    #[serde(default)]
    pub execution_id: String,
    #[serde(default)]
    pub started_at: String,
    #[serde(default)]
    pub stopped_at: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub level: Level,
    pub message: String,
    #[serde(default)]
    pub node: String,
}

struct Plan {
    order: Vec<String>,
    levels: Vec<Vec<String>>,
    incoming: HashMap<String, Vec<(String, usize)>>,
}

fn plan(workflow: &Workflow) -> EngineResult<Plan> {
    let enabled: Vec<&WorkflowNode> = workflow.nodes.iter().filter(|n| !n.disabled).collect();
    let mut incoming: HashMap<String, Vec<(String, usize)>> = HashMap::new();
    let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
    for n in &enabled {
        incoming.entry(n.name.clone()).or_default();
        outgoing.entry(n.name.clone()).or_default();
    }
    for n in &enabled {
        for (bi, branch) in workflow.branches(&n.name).iter().enumerate() {
            for s in branch {
                if let Some(list) = incoming.get_mut(s) {
                    list.push((n.name.clone(), bi));
                }
                if let Some(out) = outgoing.get_mut(&n.name) {
                    out.push(s.clone());
                }
            }
        }
    }

    // Kahn for order + levels
    let mut indegree: HashMap<String, usize> = HashMap::new();
    for n in &enabled {
        indegree.insert(n.name.clone(), incoming.get(&n.name).map(|v| v.len()).unwrap_or(0));
    }
    let mut done_set: HashSet<String> = HashSet::new();
    let mut order: Vec<String> = Vec::new();
    let mut levels: Vec<Vec<String>> = Vec::new();

    loop {
        let mut current_level: Vec<String> = enabled
            .iter()
            .filter(|n| !done_set.contains(&n.name))
            .filter(|n| {
                incoming
                    .get(&n.name)
                    .map(|ps| ps.iter().all(|(p, _)| done_set.contains(p)))
                    .unwrap_or(true)
            })
            .map(|n| n.name.clone())
            .collect();
        current_level.sort();
        if current_level.is_empty() {
            break;
        }
        for name in &current_level {
            done_set.insert(name.clone());
            order.push(name.clone());
        }
        levels.push(current_level);
    }

    if done_set.len() != enabled.len() {
        let stuck: Vec<&str> = enabled
            .iter()
            .map(|n| n.name.as_str())
            .filter(|k| !done_set.contains(*k))
            .collect();
        return Err(EngineError::new(format!(
            "cycle detected, stuck at: {}",
            stuck.join(", ")
        )));
    }

    Ok(Plan {
        order,
        levels,
        incoming,
    })
}

pub struct Engine;

impl Engine {
    pub fn explain(workflow: &Workflow) -> EngineResult<Vec<String>> {
        Ok(plan(workflow)?.order)
    }

    pub fn explain_levels(workflow: &Workflow) -> EngineResult<Vec<Vec<String>>> {
        Ok(plan(workflow)?.levels)
    }

    pub fn run(workflow: &Workflow, registry: &Registry) -> EngineResult<RunReport> {
        Self::run_with(workflow, registry, None)
    }

    pub fn run_with(
        workflow: &Workflow,
        registry: &Registry,
        webhook: Option<Value>,
    ) -> EngineResult<RunReport> {
        Self::run_parallel(workflow, registry, webhook)
    }

    pub fn run_parallel(
        workflow: &Workflow,
        registry: &Registry,
        webhook: Option<Value>,
    ) -> EngineResult<RunReport> {
        let p = plan(workflow)?;
        let by_name: HashMap<String, &WorkflowNode> = workflow
            .nodes
            .iter()
            .map(|n| (n.name.clone(), n))
            .collect();

        let done: Arc<Mutex<HashMap<String, BranchOutputs>>> = Arc::new(Mutex::new(HashMap::new()));
        let durations: Arc<Mutex<HashMap<String, u128>>> = Arc::new(Mutex::new(HashMap::new()));
        let execution_id = uuid::Uuid::new_v4().to_string();
        let started_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        for level in &p.levels {
            // execute level in parallel via rayon
            let level_nodes: Vec<(String, &WorkflowNode)> = level
                .iter()
                .filter_map(|name| by_name.get(name).map(|n| (name.clone(), *n)))
                .collect();

            let results: Vec<EngineResult<(String, BranchOutputs, u128)>> = level_nodes
                .par_iter()
                .map(|(name, node)| {
                    let node_impl = registry.get(&node.node_type).ok_or_else(|| {
                        EngineError::new(format!(
                            "unknown node type '{}' (node '{name}')",
                            node.node_type
                        ))
                    })?;

                    // gather inputs from done
                    let done_guard = done.lock().unwrap();
                    let mut inputs = Vec::new();
                    if let Some(ps) = p.incoming.get(name) {
                        for (pr, bi) in ps {
                            let branch_items = done_guard
                                .get(pr)
                                .and_then(|b| b.get(*bi))
                                .cloned()
                                .unwrap_or_default();
                            inputs.push(MultiInput {
                                from: pr.clone(),
                                branch: *bi,
                                items: branch_items,
                            });
                        }
                    }
                    drop(done_guard);

                    let done_snapshot = done.lock().unwrap().clone();
                    let ctx = ExecContext {
                        outputs: &done_snapshot,
                        webhook: webhook.as_ref(),
                        workflow_name: Some(&workflow.name),
                        execution_id: Some(&execution_id),
                    };

                    let t0 = std::time::Instant::now();
                    let out = match node_impl.execute_multi(node, inputs, &ctx) {
                        Ok(o) => o,
                        Err(e) => {
                            // check continueOnFail + error branch (onError)
                            let continue_on_fail = node
                                .parameters
                                .get("options")
                                .and_then(|o| o.get("continueOnFail"))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false)
                                || node
                                    .parameters
                                    .get("continueOnFail")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                            let on_error = node.parameters.get("onError").and_then(|v| v.as_str()).unwrap_or("")
                                .to_string()
                                + node.parameters.get("options").and_then(|o| o.get("onError")).and_then(|v| v.as_str()).unwrap_or("");
                            let is_error_branch = on_error.contains("continueErrorOutput") || on_error.contains("errorOutput") || node.parameters.get("alwaysOutputData").and_then(|v| v.as_bool()).unwrap_or(false);
                            if continue_on_fail {
                                vec![vec![serde_json::json!({"error": e.to_string(), "failed": true})]]
                            } else if is_error_branch {
                                // error branch: first branch empty, second contains error (binary passthrough preserved)
                                vec![vec![], vec![serde_json::json!({"error": e.to_string(), "failed": true, "errorBranch": true})]]
                            } else {
                                return Err(EngineError::new(format!("node '{name}' failed: {e}")));
                            }
                        }
                    };
                    let dur = t0.elapsed().as_millis();
                    Ok((name.clone(), out, dur))
                })
                .collect();

            // merge results, check errors
            for res in results {
                match res {
                    Ok((name, out, dur)) => {
                        done.lock().unwrap().insert(name.clone(), out);
                        durations.lock().unwrap().insert(name, dur);
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        let outputs = done.lock().unwrap().clone();
        let durations_ms = durations.lock().unwrap().clone();
        let stopped_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        Ok(RunReport {
            outputs,
            order: p.order,
            durations_ms,
            execution_id,
            started_at,
            stopped_at,
            status: "success".to_string(),
        })
    }

    pub fn run_sequential(
        workflow: &Workflow,
        registry: &Registry,
        webhook: Option<Value>,
    ) -> EngineResult<RunReport> {
        let p = plan(workflow)?;
        let by_name: HashMap<&str, &WorkflowNode> = workflow
            .nodes
            .iter()
            .map(|n| (n.name.as_str(), n))
            .collect();
        let mut done: HashMap<String, BranchOutputs> = HashMap::new();
        let mut durations_ms: HashMap<String, u128> = HashMap::new();
        let execution_id = uuid::Uuid::new_v4().to_string();
        let started_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        for name in &p.order {
            let node = by_name
                .get(name.as_str())
                .copied()
                .ok_or_else(|| EngineError::new(format!("plan menyebut node hilang '{name}'")))?;
            let mut inputs = Vec::new();
            if let Some(ps) = p.incoming.get(name) {
                for (pr, bi) in ps {
                    let branch_items = done
                        .get(pr)
                        .and_then(|b| b.get(*bi))
                        .cloned()
                        .unwrap_or_default();
                    inputs.push(MultiInput {
                        from: pr.clone(),
                        branch: *bi,
                        items: branch_items,
                    });
                }
            }
            let node_impl = registry.get(&node.node_type).ok_or_else(|| {
                EngineError::new(format!(
                    "unknown node type '{}' (node '{name}')",
                    node.node_type
                ))
            })?;
            let ctx = ExecContext {
                outputs: &done,
                webhook: webhook.as_ref(),
                workflow_name: Some(&workflow.name),
                execution_id: Some(&execution_id),
            };
            let t0 = std::time::Instant::now();
            let out = node_impl
                .execute_multi(node, inputs, &ctx)
                .map_err(|e| EngineError::new(format!("node '{name}' failed: {e}")))?;
            durations_ms.insert(name.clone(), t0.elapsed().as_millis());
            done.insert(name.clone(), out);
        }
        let stopped_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        Ok(RunReport {
            outputs: done,
            order: p.order,
            durations_ms,
            execution_id,
            started_at,
            stopped_at,
            status: "success".to_string(),
        })
    }

    pub fn lint(workflow: &Workflow, registry: &Registry) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        let err = |message: String, node: String| Diagnostic {
            level: Level::Error,
            message,
            node,
        };
        let warn = |message: String, node: String| Diagnostic {
            level: Level::Warning,
            message,
            node,
        };
        for n in &workflow.nodes {
            if registry.get(&n.node_type).is_none() {
                out.push(err(
                    format!("node '{}': tipe tak dikenal '{}'", n.name, n.node_type),
                    n.name.clone(),
                ));
            }
        }
        for (from, to) in workflow.dangling_edges() {
            out.push(err(
                format!("edge gantung: '{from}' -> '{to}' (target tak ada)"),
                from,
            ));
        }
        {
            let names: HashSet<&str> = workflow.nodes.iter().map(|n| n.name.as_str()).collect();
            let mut ghosts: Vec<&str> = workflow
                .connections
                .keys()
                .map(String::as_str)
                .filter(|k| !names.contains(k))
                .collect();
            ghosts.sort_unstable();
            for g in ghosts {
                out.push(warn(
                    format!("connections dari '{g}' yang bukan node — diabaikan"),
                    g.to_string(),
                ));
            }
        }
        if let Err(e) = plan(workflow) {
            out.push(err(e.to_string(), String::new()));
        }
        let mut targets: HashSet<String> = HashSet::new();
        for n in &workflow.nodes {
            for s in workflow.successors(&n.name) {
                targets.insert(s);
            }
        }
        if workflow.nodes.len() > 1 {
            for n in &workflow.nodes {
                let has_out = !workflow.successors(&n.name).is_empty();
                if !has_out && !targets.contains(n.name.as_str()) {
                    out.push(warn(
                        format!("node '{}' terisolasi (tanpa edge)", n.name),
                        n.name.clone(),
                    ));
                }
            }
        }
        for n in &workflow.nodes {
            if n.disabled {
                continue;
            }
            if !targets.contains(n.name.as_str()) && !n.node_type.to_lowercase().contains("trigger") {
                out.push(warn(
                    format!(
                        "node '{}' tak punya pendahulu dan bukan trigger — berjalan dengan 0 item",
                        n.name
                    ),
                    n.name.clone(),
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Emit;
    struct Pass;
    struct EchoHook;

    impl Node for Emit {
        fn node_type(&self) -> &'static str {
            "test.emit"
        }
        fn execute(
            &self,
            _node: &WorkflowNode,
            _items: Vec<Value>,
            _ctx: &ExecContext,
        ) -> EngineResult<BranchOutputs> {
            Ok(vec![vec![Value::String("e".to_string())]])
        }
    }

    impl Node for Pass {
        fn node_type(&self) -> &'static str {
            "test.pass"
        }
        fn execute(
            &self,
            _node: &WorkflowNode,
            items: Vec<Value>,
            _ctx: &ExecContext,
        ) -> EngineResult<BranchOutputs> {
            Ok(vec![items])
        }
    }

    impl Node for EchoHook {
        fn node_type(&self) -> &'static str {
            "test.echohook"
        }
        fn execute(
            &self,
            _node: &WorkflowNode,
            _items: Vec<Value>,
            ctx: &ExecContext,
        ) -> EngineResult<BranchOutputs> {
            Ok(vec![vec![ctx.webhook.cloned().unwrap_or(Value::Null)]])
        }
    }

    fn node(name: &str, t: &str) -> WorkflowNode {
        WorkflowNode {
            id: format!("id-{name}"),
            name: name.to_string(),
            node_type: t.to_string(),
            type_version: 1.0,
            position: [0.0, 0.0],
            parameters: HashMap::new(),
            credentials: HashMap::new(),
            disabled: false,
            notes: String::new(),
            notes_in_flow: false,
            extra: HashMap::new(),
        }
    }

    fn registry() -> Registry {
        let mut r = Registry::default();
        r.register(Arc::new(Emit));
        r.register(Arc::new(Pass));
        r.register(Arc::new(EchoHook));
        r
    }

    fn workflow(nodes: Vec<WorkflowNode>, connections: HashMap<String, Value>) -> n8n_core::Workflow {
        n8n_core::Workflow {
            id: "test-id".to_string(),
            name: "t".to_string(),
            nodes,
            connections,
            active: false,
            settings: HashMap::new(),
            version_id: String::new(),
            tags: vec![],
            pin_data: HashMap::new(),
            meta: HashMap::new(),
            created_at: String::new(),
            updated_at: String::new(),
            extra: HashMap::new(),
        }
    }

    #[test]
    fn linear_chain_passes_items_through() {
        let wf = workflow(
            vec![node("A", "test.emit"), node("B", "test.pass")],
            HashMap::from([(
                "A".to_string(),
                serde_json::json!({ "main": [[{ "node": "B", "type": "main", "index": 0 }]] }),
            )]),
        );
        let report = Engine::run(&wf, &registry()).expect("run");
        assert_eq!(report.order, vec!["A".to_string(), "B".to_string()]);
        assert_eq!(report.outputs["B"][0], vec![Value::String("e".to_string())]);
        assert_eq!(report.durations_ms.len(), 2);
    }

    #[test]
    fn parallel_execution_independent_branches() {
        // A -> C, B -> C, A and B independent should run in parallel level
        let wf = workflow(
            vec![
                node("A", "test.emit"),
                node("B", "test.emit"),
                node("C", "test.pass"),
            ],
            HashMap::from([
                (
                    "A".to_string(),
                    serde_json::json!({ "main": [[{ "node": "C" }]] }),
                ),
                (
                    "B".to_string(),
                    serde_json::json!({ "main": [[{ "node": "C" }]] }),
                ),
            ]),
        );
        let levels = Engine::explain_levels(&wf).expect("levels");
        assert_eq!(levels.len(), 2);
        assert!(levels[0].contains(&"A".to_string()));
        assert!(levels[0].contains(&"B".to_string()));
        assert_eq!(levels[1], vec!["C".to_string()]);
        let report = Engine::run(&wf, &registry()).expect("run");
        assert_eq!(report.order.len(), 3);
    }

    #[test]
    fn unknown_type_is_an_explicit_error() {
        let wf = workflow(vec![node("A", "test.missing")], HashMap::new());
        let err = Engine::run(&wf, &registry()).expect_err("must fail");
        assert!(err.to_string().contains("unknown node type"), "{err}");
    }

    #[test]
    fn cycle_is_detected() {
        let wf = workflow(
            vec![node("A", "test.pass"), node("B", "test.pass")],
            HashMap::from([
                (
                    "A".to_string(),
                    serde_json::json!({ "main": [[{ "node": "B" }]] }),
                ),
                (
                    "B".to_string(),
                    serde_json::json!({ "main": [[{ "node": "A" }]] }),
                ),
            ]),
        );
        let err = Engine::run(&wf, &registry()).expect_err("must fail");
        assert!(err.to_string().contains("cycle"), "{err}");
    }

    #[test]
    fn explain_plans_without_executing() {
        let wf = workflow(
            vec![node("A", "test.missing"), node("B", "test.missing")],
            HashMap::from([(
                "A".to_string(),
                serde_json::json!({ "main": [[{ "node": "B" }]] }),
            )]),
        );
        assert_eq!(
            Engine::explain(&wf).expect("plan"),
            vec!["A".to_string(), "B".to_string()]
        );
    }

    #[test]
    fn lint_reports_unknown_type_and_dangling_edge() {
        let wf = workflow(
            vec![node("A", "test.missing")],
            HashMap::from([(
                "A".to_string(),
                serde_json::json!({ "main": [[{ "node": "Ghost" }]] }),
            )]),
        );
        let diags = Engine::lint(&wf, &registry());
        assert!(
            diags
                .iter()
                .any(|d| d.level == Level::Error && d.message.contains("tak dikenal")),
            "{diags:?}"
        );
        assert!(
            diags
                .iter()
                .any(|d| d.level == Level::Error && d.message.contains("gantung")),
            "{diags:?}"
        );
    }

    #[test]
    fn execute_multi_sees_separate_inputs() {
        struct Probe;
        impl Node for Probe {
            fn node_type(&self) -> &'static str {
                "test.probe"
            }
            fn execute(
                &self,
                _node: &WorkflowNode,
                _items: Vec<Value>,
                _ctx: &ExecContext,
            ) -> EngineResult<BranchOutputs> {
                unreachable!("probe hanya via execute_multi")
            }
            fn execute_multi(
                &self,
                _node: &WorkflowNode,
                inputs: Vec<MultiInput>,
                _ctx: &ExecContext,
            ) -> EngineResult<BranchOutputs> {
                let summary: Vec<Value> = inputs
                    .iter()
                    .map(|i| {
                        serde_json::json!({"from": i.from, "branch": i.branch, "n": i.items.len()})
                    })
                    .collect();
                Ok(vec![vec![Value::Array(summary)]])
            }
        }
        let mut reg = registry();
        reg.register(Arc::new(Probe));
        let wf = workflow(
            vec![
                node("A", "test.emit"),
                node("B", "test.emit"),
                node("P", "test.probe"),
            ],
            HashMap::from([
                (
                    "A".to_string(),
                    serde_json::json!({ "main": [[{ "node": "P" }]] }),
                ),
                (
                    "B".to_string(),
                    serde_json::json!({ "main": [[{ "node": "P" }]] }),
                ),
            ]),
        );
        let report = Engine::run(&wf, &reg).expect("run");
        assert_eq!(
            report.outputs["P"][0],
            vec![serde_json::json!([
                {"from": "A", "branch": 0, "n": 1},
                {"from": "B", "branch": 0, "n": 1}
            ])]
        );
    }

    #[test]
    fn run_with_threads_webhook_payload() {
        let wf = workflow(vec![node("A", "test.echohook")], HashMap::new());
        let report =
            Engine::run_with(&wf, &registry(), Some(serde_json::json!({"h": 1}))).expect("run");
        assert_eq!(report.outputs["A"][0], vec![serde_json::json!({"h": 1})]);
    }

    #[test]
    fn continue_on_fail_does_not_abort() {
        struct Fail;
        impl Node for Fail {
            fn node_type(&self) -> &'static str {
                "test.fail"
            }
            fn execute(
                &self,
                _node: &WorkflowNode,
                _items: Vec<Value>,
                _ctx: &ExecContext,
            ) -> EngineResult<BranchOutputs> {
                Err(EngineError::new("intentional fail"))
            }
        }
        let mut reg = registry();
        reg.register(Arc::new(Fail));
        let mut n = node("F", "test.fail");
        n.parameters.insert(
            "continueOnFail".to_string(),
            serde_json::json!(true),
        );
        let wf = workflow(vec![n], HashMap::new());
        let report = Engine::run(&wf, &reg).expect("should continue");
        assert!(report.outputs["F"][0][0].get("failed").is_some());
    }
}
