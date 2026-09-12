//! n8n-cli: validate / explain / run / nodes.
//! Argumen diurai manual (tanpa clap — ringan).

use n8n_core::Workflow;
use n8n_engine::{Engine, Level, Registry};
use n8n_nodes::register_all;
use std::process::ExitCode;

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("validate") => cmd_validate(args.get(2)),
        Some("explain") => cmd_explain(args.get(2)),
        Some("run") => {
            let save = args
                .iter()
                .position(|a| a == "--save")
                .and_then(|i| args.get(i + 1).map(String::as_str));
            cmd_run(args.get(2), save)
        }
        Some("nodes") => cmd_nodes(),
        _ => {
            usage();
            Err("perintah tak dikenal".to_string())
        }
    }
}

fn usage() {
    eprintln!(
        "pakai: n8n-cli validate <workflow.json> | explain <workflow.json> | run <workflow.json> [--save <report.json>] | nodes"
    );
}

fn need_path(arg: Option<&String>) -> Result<&str, String> {
    arg.map(String::as_str)
        .ok_or_else(|| "kurang argumen: path workflow.json".to_string())
}

fn load_workflow(path: &str) -> Result<Workflow, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("baca '{path}' gagal: {e}"))?;
    Workflow::from_json(&raw).map_err(|e| format!("parse '{path}' gagal: {e}"))
}

fn full_registry() -> Registry {
    let mut registry = Registry::default();
    register_all(&mut registry);
    registry
}

fn cmd_validate(arg: Option<&String>) -> Result<(), String> {
    let wf = load_workflow(need_path(arg)?)?;
    let diags = Engine::lint(&wf, &full_registry());
    let errors = diags.iter().filter(|d| d.level == Level::Error).count();
    if diags.is_empty() {
        println!(
            "OK: '{}' bersih ({} node, {} edge)",
            wf.name,
            wf.nodes.len(),
            wf.edge_count()
        );
    } else {
        for d in &diags {
            let tag = match d.level {
                Level::Error => "ERROR",
                Level::Warning => "WARN",
            };
            println!("[{tag}] {}", d.message);
        }
    }
    if errors > 0 {
        return Err(format!("{errors} error"));
    }
    Ok(())
}

fn cmd_explain(arg: Option<&String>) -> Result<(), String> {
    let wf = load_workflow(need_path(arg)?)?;
    match Engine::explain(&wf) {
        Ok(order) => {
            println!("rencana: {}", order.join(" -> "));
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}

fn cmd_run(arg: Option<&String>, save: Option<&str>) -> Result<(), String> {
    let wf = load_workflow(need_path(arg)?)?;
    let report = Engine::run(&wf, &full_registry()).map_err(|e| e.to_string())?;
    println!("urutan: {}", report.order.join(" -> "));
    let total: u128 = report.durations_ms.values().sum();
    let per: Vec<String> = report
        .order
        .iter()
        .map(|n| format!("{n}={}ms", report.durations_ms.get(n).copied().unwrap_or(0)))
        .collect();
    println!("waktu: {} (total {total}ms)", per.join(", "));
    let out = serde_json::to_string_pretty(&report.outputs).map_err(|e| e.to_string())?;
    println!("{out}");
    if let Some(path) = save {
        let pretty = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
        std::fs::write(path, pretty).map_err(|e| format!("simpan '{path}' gagal: {e}"))?;
        println!("tersimpan: {path}");
    }
    Ok(())
}

fn cmd_nodes() -> Result<(), String> {
    for t in full_registry().types() {
        println!("{t}");
    }
    Ok(())
}
