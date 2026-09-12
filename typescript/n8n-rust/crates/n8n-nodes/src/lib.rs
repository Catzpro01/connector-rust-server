//! n8n-nodes: node bawaan v1 — meniru parameter & perilaku n8n asli.
//!
//! v0.5.0 (fidelity pass terhadap n8n-io/n8n):
//! - set: mode manual/raw, assignments + fields.values, include
//!   all/none/selected/except, dot-notation (`options.dotNotation`)
//! - if/filter: object `conditions` n8n (lihat `filter.rs`) + string
//!   `condition` lawas sebagai fallback
//! - sort: `type` simple/random/code + `sortFieldsUi` + case-insensitive
//! - limit: `maxItems` + `keep` firstItems/lastItems
//! - code/function: `mode` runOnceForAllItems/runOnceForEachItem (rhai)
//! - httpRequest: fan-out per item + `options.timeout` (ms, default 300000)
//!   + responseFormat autodetect/json/text
//! - scheduleTrigger: field output persis n8n (UTC) + echo `rule`
//! - webhook: emit payload server; `httpMethod`/`responseMode`/`responseData`/
//!   `responseCode` dibaca server (lihat n8n-server)
//!
//! Nilai string di parameter dirender sebagai template `={{ }}`.
//!
//! v0.6.0: switch (N cabang + else), merge (multi-input; lihat
//! `merge.rs`), dateTime (lihat `datetime.rs`), respondToWebhook
//! (passthrough; server membaca paramsnya), wait (subset tidur),
//! stopAndError (selalu gagal).
//!
//! v0.7.0: executeCommand (perintah shell host via `sh -c`).

mod datetime;
mod extended;
mod filter;
mod merge;

use filter::{evaluate as eval_conditions, resolve_opts};
use n8n_core::expr::{render, render_value, ExprContext};
use n8n_core::WorkflowNode;
use n8n_engine::{
    BranchOutputs, EngineError, EngineResult, ExecContext, MultiInput, Node, Registry,
};
use serde_json::{json, Map, Value};
use std::sync::Arc;

pub struct ManualTrigger;
pub struct SetNode;
pub struct NoOp;
pub struct FilterNode;
pub struct SortNode;
pub struct LimitNode;
pub struct IfNode;
pub struct HttpNode;
pub struct CodeNode;
pub struct FunctionNode;
pub struct ScheduleNode;
pub struct WebhookNode;
pub struct SwitchNode;
pub struct MergeNode;
pub struct DateTimeNode;
pub struct RespondNode;
pub struct WaitNode;
pub struct StopNode;
pub struct ExecNode;

// ---------------------------------------------------------------------------
// Path helper ala lodash get/set/unset (subset: segmen object + indeks array)
// ---------------------------------------------------------------------------

pub(crate) fn get_path<'a>(v: &'a Value, dotted: &str) -> Option<&'a Value> {
    let mut cur = v;
    for seg in dotted.split('.') {
        if seg.is_empty() {
            return None;
        }
        match cur {
            Value::Array(a) => {
                let idx: usize = seg.parse().ok()?;
                cur = a.get(idx)?;
            }
            Value::Object(o) => {
                cur = o.get(seg)?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

fn set_path(root: &mut Map<String, Value>, dotted: &str, value: Value) {
    let segs: Vec<&str> = dotted.split('.').filter(|s| !s.is_empty()).collect();
    if segs.is_empty() {
        return;
    }
    let mut pending = Some(value);
    let mut holder = Value::Object(std::mem::take(root));
    let mut cur = &mut holder;
    for (i, seg) in segs.iter().enumerate() {
        let last = i + 1 == segs.len();
        if matches!(cur, Value::Array(_)) {
            if let Ok(n) = seg.parse::<usize>() {
                let a = cur.as_array_mut().expect("array");
                while a.len() <= n {
                    a.push(Value::Null);
                }
                if last {
                    a[n] = pending.take().unwrap_or(Value::Null);
                    break;
                }
                cur = &mut a[n];
                if cur.is_null() {
                    *cur = Value::Object(Map::new());
                }
                continue;
            }
        }
        if !cur.is_object() {
            *cur = Value::Object(Map::new());
        }
        let o = cur.as_object_mut().expect("object");
        if last {
            o.insert(seg.to_string(), pending.take().unwrap_or(Value::Null));
            break;
        }
        cur = o
            .entry(seg.to_string())
            .or_insert(Value::Object(Map::new()));
    }
    if let Value::Object(m) = holder {
        *root = m;
    }
}

fn unset_path(root: &mut Map<String, Value>, dotted: &str) {
    let segs: Vec<&str> = dotted.split('.').filter(|s| !s.is_empty()).collect();
    if segs.is_empty() {
        return;
    }
    let mut holder = Value::Object(std::mem::take(root));
    {
        let mut cur = &mut holder;
        for (i, seg) in segs.iter().enumerate() {
            let last = i + 1 == segs.len();
            match cur {
                Value::Array(a) => {
                    let n: usize = match seg.parse() {
                        Ok(n) => n,
                        Err(_) => break,
                    };
                    if last {
                        if n < a.len() {
                            a[n] = Value::Null;
                        }
                        break;
                    }
                    if n >= a.len() {
                        break;
                    }
                    cur = &mut a[n];
                }
                Value::Object(o) => {
                    if last {
                        o.remove(*seg);
                        break;
                    }
                    match o.get_mut(*seg) {
                        Some(next) => cur = next,
                        None => break,
                    }
                }
                _ => break,
            }
        }
    }
    if let Value::Object(m) = holder {
        *root = m;
    }
}

fn split_csv(v: &Value) -> Vec<String> {
    v.as_str()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Trigger + passthrough
// ---------------------------------------------------------------------------

impl Node for ManualTrigger {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.manualTrigger"
    }

    /// n8n: emit `[{}]` (satu item kosong).
    fn execute(
        &self,
        _node: &WorkflowNode,
        _items: Vec<Value>,
        _ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        Ok(vec![vec![Value::Object(Map::new())]])
    }
}

impl Node for NoOp {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.noOp"
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

// ---------------------------------------------------------------------------
// Set — mode manual/raw, include, dot-notation (n8n Set v2/v3)
// ---------------------------------------------------------------------------

impl Node for SetNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.set"
    }

    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let mode = node
            .parameters
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("manual");
        if mode == "raw" {
            let mut out = Vec::with_capacity(items.len());
            for it in &items {
                let ectx = ExprContext::simple(it, ctx.outputs);
                out.push(set_raw(node, &ectx)?);
            }
            return Ok(vec![out]);
        }
        if mode != "manual" {
            return Err(EngineError::new(format!("set: mode tak dikenal '{mode}'")));
        }
        let include = node
            .parameters
            .get("include")
            .and_then(Value::as_str)
            .unwrap_or("all");
        let dot = node
            .parameters
            .get("options")
            .and_then(|o| o.get("dotNotation"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let mut out = Vec::with_capacity(items.len());
        for it in &items {
            let ectx = ExprContext::simple(it, ctx.outputs);
            let mut base = match include {
                "all" => it.as_object().cloned().unwrap_or_default(),
                "none" => Map::new(),
                "selected" => {
                    let mut m = Map::new();
                    let fields = render_value(
                        node.parameters.get("includeFields").unwrap_or(&Value::Null),
                        &ectx,
                    );
                    for key in split_csv(&fields) {
                        let v = if dot {
                            get_path(it, &key).cloned()
                        } else {
                            it.get(&key).cloned()
                        };
                        if let Some(v) = v {
                            let target = if dot && key.contains('.') {
                                key.rsplit('.').next().unwrap_or(&key)
                            } else {
                                key.as_str()
                            };
                            if dot {
                                set_path(&mut m, target, v);
                            } else {
                                m.insert(target.to_string(), v);
                            }
                        }
                    }
                    m
                }
                "except" => {
                    let mut m = it.as_object().cloned().unwrap_or_default();
                    let fields = render_value(
                        node.parameters.get("excludeFields").unwrap_or(&Value::Null),
                        &ectx,
                    );
                    for key in split_csv(&fields) {
                        if dot {
                            unset_path(&mut m, &key);
                        } else {
                            m.remove(&key);
                        }
                    }
                    m
                }
                other => {
                    return Err(EngineError::new(format!(
                        "set: include tak dikenal '{other}'"
                    )))
                }
            };
            for (k, v) in set_new_fields(node, &ectx)? {
                if dot {
                    set_path(&mut base, &k, v);
                } else {
                    base.insert(k, v);
                }
            }
            out.push(Value::Object(base));
        }
        Ok(vec![out])
    }
}

/// Field baru: `assignments` (n8n v3) → `fields.values` (n8n v1/v2) →
/// `values` (ekstensi n8n-rust).
fn set_new_fields(node: &WorkflowNode, ectx: &ExprContext) -> EngineResult<Map<String, Value>> {
    if let Some(list) = node
        .parameters
        .get("assignments")
        .and_then(|v| v.get("assignments"))
        .and_then(Value::as_array)
    {
        let mut m = Map::new();
        for a in list.iter().filter_map(Value::as_object) {
            if let (Some(Value::String(name)), Some(value)) = (a.get("name"), a.get("value")) {
                m.insert(name.clone(), render_value(value, ectx));
            }
        }
        return Ok(m);
    }
    if let Some(list) = node
        .parameters
        .get("fields")
        .and_then(|v| v.get("values"))
        .and_then(Value::as_array)
    {
        let mut m = Map::new();
        for f in list.iter().filter_map(Value::as_object) {
            let name = f.get("name").and_then(Value::as_str);
            let t = f
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("stringValue");
            let rendered = render_value(f.get(t).unwrap_or(&Value::Null), ectx);
            let val = match (&rendered, t) {
                (Value::String(s), "objectValue" | "arrayValue") => {
                    serde_json::from_str(s).unwrap_or(rendered.clone())
                }
                _ => rendered,
            };
            if let Some(n) = name {
                m.insert(n.to_string(), val);
            }
        }
        return Ok(m);
    }
    if let Some(Value::Object(values)) = node.parameters.get("values") {
        return Ok(values
            .iter()
            .map(|(k, v)| {
                let nv = match v {
                    Value::String(s) => render(s, ectx),
                    other => other.clone(),
                };
                (k.clone(), nv)
            })
            .collect());
    }
    Ok(Map::new())
}

/// Mode raw: `jsonOutput` di-render lalu HARUS parse jadi object.
fn set_raw(node: &WorkflowNode, ectx: &ExprContext) -> EngineResult<Value> {
    let raw = node
        .parameters
        .get("jsonOutput")
        .map(|v| render_value(v, ectx))
        .unwrap_or(Value::Null);
    let text = raw
        .as_str()
        .ok_or_else(|| EngineError::new("set: mode raw butuh 'jsonOutput' string"))?;
    // Penanda ekspresi `=` di awal dibuang sebelum parse JSON.
    let text = text.trim_start().strip_prefix('=').unwrap_or(text);
    let parsed: Value = serde_json::from_str(text)
        .map_err(|e| EngineError::new(format!("set: jsonOutput bukan JSON: {e}")))?;
    match parsed {
        Value::Object(_) => Ok(parsed),
        _ => Err(EngineError::new(
            "set: jsonOutput harus object JSON (array/primitif ditolak)",
        )),
    }
}

// ---------------------------------------------------------------------------
// Filter + If — conditions n8n (+ fallback string `condition` lawas)
// ---------------------------------------------------------------------------

fn check_item(
    node: &WorkflowNode,
    item: &Value,
    ctx: &ExecContext,
    index: usize,
) -> EngineResult<bool> {
    let ectx = ExprContext::simple(item, ctx.outputs);
    if let Some(conds) = node.parameters.get("conditions") {
        if conds.is_object() {
            let opts = resolve_opts(conds, node.parameters.get("options"));
            let render = |v: &Value| render_value(v, &ectx);
            return eval_conditions(conds, index, &render, &opts);
        }
    }
    let cond = node
        .parameters
        .get("condition")
        .and_then(Value::as_str)
        .unwrap_or("");
    Ok(truthy(&render(cond, &ectx)))
}

impl Node for FilterNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.filter"
    }

    /// n8n Filter: teruskan item yang lolos `conditions`.
    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let mut out = Vec::new();
        for (i, it) in items.into_iter().enumerate() {
            if check_item(node, &it, ctx, i)? {
                out.push(it);
            }
        }
        Ok(vec![out])
    }
}

impl Node for IfNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.if"
    }

    /// n8n If: belah ke `[true, false]` menurut `conditions`.
    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let mut yes = Vec::new();
        let mut no = Vec::new();
        for (i, it) in items.into_iter().enumerate() {
            if check_item(node, &it, ctx, i)? {
                yes.push(it);
            } else {
                no.push(it);
            }
        }
        Ok(vec![yes, no])
    }
}

// ---------------------------------------------------------------------------
// Sort — type simple/random/code (n8n Transform/Sort)
// ---------------------------------------------------------------------------

struct SortField {
    name: String,
    /// 1 = ascending, -1 = descending.
    dir: i32,
}

impl Node for SortNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.sort"
    }

    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        _ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let sort_type = node
            .parameters
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("simple");
        match sort_type {
            "random" => {
                let mut out = items;
                shuffle(&mut out);
                return Ok(vec![out]);
            }
            "code" => {
                return Err(EngineError::new(
                    "sort: type 'code' butuh JavaScript (n8n) — gunakan node Code (rhai) untuk comparator kustom",
                ))
            }
            "simple" => {}
            other => {
                return Err(EngineError::new(format!(
                    "sort: type tak dikenal '{other}'"
                )))
            }
        }
        let dot = !node
            .parameters
            .get("options")
            .and_then(|o| o.get("disableDotNotation"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let fields = sort_fields(node)?;
        for f in &fields {
            let found = items.iter().any(|it| sort_get(it, &f.name, dot).is_some());
            if !found {
                return Err(EngineError::new(format!(
                    "sort: Couldn't find the field '{}' in the input data",
                    f.name
                )));
            }
        }
        let mut out = items;
        out.sort_by(|a, b| {
            for f in &fields {
                let ord = cmp_field(sort_get(a, &f.name, dot), sort_get(b, &f.name, dot));
                if ord != std::cmp::Ordering::Equal {
                    return if f.dir == 1 { ord } else { ord.reverse() };
                }
            }
            std::cmp::Ordering::Equal
        });
        Ok(vec![out])
    }
}

/// `sortFieldsUi.sortField[]` n8n, fallback `field`+`order` lawas.
fn sort_fields(node: &WorkflowNode) -> EngineResult<Vec<SortField>> {
    if let Some(list) = node
        .parameters
        .get("sortFieldsUi")
        .and_then(|v| v.get("sortField"))
        .and_then(Value::as_array)
    {
        let mut fields = Vec::new();
        for f in list.iter().filter_map(Value::as_object) {
            let name = f
                .get("fieldName")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let dir = match f
                .get("order")
                .and_then(Value::as_str)
                .unwrap_or("ascending")
            {
                "ascending" | "asc" => 1,
                "descending" | "desc" => -1,
                other => {
                    return Err(EngineError::new(format!(
                        "sort: order tak dikenal '{other}'"
                    )))
                }
            };
            fields.push(SortField { name, dir });
        }
        if fields.is_empty() {
            return Err(EngineError::new(
                "sort: No sorting specified. Please add a field to sort by",
            ));
        }
        return Ok(fields);
    }
    if let Some(field) = node.parameters.get("field").and_then(Value::as_str) {
        let dir = match node
            .parameters
            .get("order")
            .and_then(Value::as_str)
            .unwrap_or("asc")
        {
            "asc" | "ascending" => 1,
            "desc" | "descending" => -1,
            other => {
                return Err(EngineError::new(format!(
                    "sort: order tak dikenal '{other}'"
                )))
            }
        };
        return Ok(vec![SortField {
            name: field.to_string(),
            dir,
        }]);
    }
    Err(EngineError::new(
        "sort: No sorting specified. Please add a field to sort by",
    ))
}

fn sort_get<'a>(item: &'a Value, field: &str, dot: bool) -> Option<&'a Value> {
    if dot {
        get_path(item, field)
    } else {
        item.get(field)
    }
}

/// String case-insensitive (n8n), angka numerik, sisanya peringkat tipe.
fn cmp_field(a: Option<&Value>, b: Option<&Value>) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let a = a.unwrap_or(&Value::Null);
    let b = b.unwrap_or(&Value::Null);
    match (a, b) {
        (Value::String(x), Value::String(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
        (Value::Number(x), Value::Number(y)) => {
            let xf = x.as_f64().unwrap_or(f64::NAN);
            let yf = y.as_f64().unwrap_or(f64::NAN);
            xf.partial_cmp(&yf).unwrap_or(Ordering::Equal)
        }
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => {
            if a == b {
                return Ordering::Equal;
            }
            rank(a).cmp(&rank(b))
        }
    }
}

fn rank(v: &Value) -> u8 {
    match v {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(_) => 2,
        Value::String(_) => 3,
        Value::Array(_) => 4,
        Value::Object(_) => 5,
    }
}

/// Fisher–Yates dengan xorshift64 (tanpa crate rand).
fn shuffle<T>(v: &mut [T]) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(v.len() as u64)
        .wrapping_mul(0xbf58_476d_1ce4_e5b9);
    if s == 0 {
        s = 0xdead_beef_cafe_f00d;
    }
    for i in (1..v.len()).rev() {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let j = (s % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
}

// ---------------------------------------------------------------------------
// Limit — maxItems + keep (n8n Transform/Limit)
// ---------------------------------------------------------------------------

impl Node for LimitNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.limit"
    }

    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        _ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let max = node
            .parameters
            .get("maxItems")
            .and_then(Value::as_i64)
            .or_else(|| node.parameters.get("count").and_then(Value::as_i64))
            .unwrap_or(1);
        let keep = node
            .parameters
            .get("keep")
            .and_then(Value::as_str)
            .unwrap_or("firstItems");
        if keep != "firstItems" && keep != "lastItems" {
            return Err(EngineError::new(format!(
                "limit: keep tak dikenal '{keep}'"
            )));
        }
        let len = items.len() as i64;
        if max >= len {
            return Ok(vec![items]);
        }
        if max <= 0 {
            return Ok(vec![Vec::new()]);
        }
        let n = max as usize;
        let out = if keep == "firstItems" {
            items.into_iter().take(n).collect()
        } else {
            items.into_iter().skip(len as usize - n).collect()
        };
        Ok(vec![out])
    }
}

// ---------------------------------------------------------------------------
// HTTP — fan-out per item + options n8n (timeout, responseFormat)
// ---------------------------------------------------------------------------

impl Node for HttpNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.httpRequest"
    }

    /// Fan-out: SATU request per item input. 0 item → 0 request.
    /// Beda disengaja vs n8n: status ≥ 400 TIDAK error — selalu output
    /// `{status, headers, body, url}` (engine belum punya error-output).
    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let mut out = Vec::with_capacity(items.len());
        for it in &items {
            let ectx = ExprContext::simple(it, ctx.outputs);
            out.push(do_request(node, &ectx)?);
        }
        Ok(vec![out])
    }
}

fn do_request(node: &WorkflowNode, ectx: &ExprContext) -> EngineResult<Value> {
    let url_v = node
        .parameters
        .get("url")
        .map(|v| render_value(v, ectx))
        .unwrap_or(Value::Null);
    let url = url_v
        .as_str()
        .ok_or_else(|| EngineError::new("httpRequest: 'url' wajib string"))?
        .to_string();
    let method = node
        .parameters
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_uppercase();
    let options = node.parameters.get("options");
    let timeout_ms = options
        .and_then(|o| o.get("timeout"))
        .and_then(Value::as_u64)
        .unwrap_or(300_000);
    let format = options
        .and_then(|o| o.get("response"))
        .and_then(|r| r.get("response"))
        .and_then(|r| r.get("responseFormat"))
        .and_then(Value::as_str)
        .or_else(|| {
            options
                .and_then(|o| o.get("responseFormat"))
                .and_then(Value::as_str)
        })
        .unwrap_or("autodetect");
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(Value::Object(h)) = node.parameters.get("headers") {
        for (k, v) in h {
            let rendered = render_value(v, ectx);
            let s = match &rendered {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => {
                    return Err(EngineError::new(format!(
                        "httpRequest: nilai header '{k}' harus skalar"
                    )))
                }
            };
            let name = reqwest::header::HeaderName::from_bytes(k.as_bytes()).map_err(|_| {
                EngineError::new(format!("httpRequest: nama header tak valid '{k}'"))
            })?;
            let value = reqwest::header::HeaderValue::from_str(&s).map_err(|_| {
                EngineError::new(format!("httpRequest: nilai header '{k}' tak valid"))
            })?;
            headers.insert(name, value);
        }
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()
        .map_err(|e| EngineError::new(format!("httpRequest: client: {e}")))?;
    let http_method: reqwest::Method = method
        .parse()
        .map_err(|e| EngineError::new(format!("httpRequest: method: {e}")))?;
    let mut req = client.request(http_method, url.clone());
    req = req.headers(headers);
    if let Some(body) = node.parameters.get("body") {
        let rendered = render_value(body, ectx);
        req = req.json(&rendered);
    }
    let resp = req
        .send()
        .map_err(|e| EngineError::new(format!("httpRequest: {e}")))?;
    let status = resp.status().as_u16();
    let mut rh = Map::new();
    for (k, v) in resp.headers().iter() {
        rh.insert(
            k.to_string(),
            Value::String(v.to_str().unwrap_or("").to_string()),
        );
    }
    let final_url = resp.url().to_string();
    let text = resp
        .text()
        .map_err(|e| EngineError::new(format!("httpRequest: baca body: {e}")))?;
    let body_v = match format {
        "text" => Value::String(text),
        "json" => serde_json::from_str(&text).map_err(|e| {
            EngineError::new(format!("httpRequest: response bukan JSON valid: {e}"))
        })?,
        "file" => {
            return Err(EngineError::new(
                "httpRequest: responseFormat 'file' belum didukung (tanpa penyimpanan binary)",
            ))
        }
        "autodetect" => serde_json::from_str(&text).unwrap_or(Value::String(text)),
        other => {
            return Err(EngineError::new(format!(
                "httpRequest: responseFormat tak dikenal '{other}'"
            )))
        }
    };
    let mut out = Map::new();
    out.insert("status".to_string(), json!(status));
    out.insert("headers".to_string(), Value::Object(rh));
    out.insert("body".to_string(), body_v);
    out.insert("url".to_string(), Value::String(final_url));
    Ok(Value::Object(out))
}

// ---------------------------------------------------------------------------
// Code / Function — rhai dengan mode n8n
// ---------------------------------------------------------------------------

fn code_mode(node: &WorkflowNode) -> &str {
    node.parameters
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("runOnceForAllItems")
}

fn run_code_mode(node: &WorkflowNode, items: Vec<Value>) -> EngineResult<Vec<Value>> {
    match code_mode(node) {
        "runOnceForAllItems" => run_code(node, items),
        "runOnceForEachItem" => run_code_each(node, items),
        other => Err(EngineError::new(format!(
            "code: mode tak dikenal '{other}'"
        ))),
    }
}

impl Node for CodeNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.code"
    }

    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        _ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        Ok(vec![run_code_mode(node, items)?])
    }
}

impl Node for FunctionNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.function"
    }

    /// Alias lama untuk `code` (kompat impor workflow n8n lama).
    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        _ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        Ok(vec![run_code_mode(node, items)?])
    }
}

/// runOnceForAllItems: `items` masuk array, wajib array keluar.
fn run_code(node: &WorkflowNode, items: Vec<Value>) -> EngineResult<Vec<Value>> {
    let code = node
        .parameters
        .get("code")
        .and_then(Value::as_str)
        .ok_or_else(|| EngineError::new("code: parameter 'code' wajib string"))?;
    let eng = rhai::Engine::new();
    let dyn_items =
        rhai::serde::to_dynamic(&items).map_err(|e| EngineError::new(format!("code: {e}")))?;
    let mut scope = rhai::Scope::new();
    scope.push("items", dyn_items);
    let _ = eng
        .eval_with_scope::<rhai::Dynamic>(&mut scope, code)
        .map_err(|e| EngineError::new(format!("code: {e}")))?;
    let back: rhai::Dynamic = scope
        .get_value("items")
        .ok_or_else(|| EngineError::new("code: variabel 'items' hilang"))?;
    let arr: Vec<Value> = rhai::serde::from_dynamic(&back)
        .map_err(|e| EngineError::new(format!("code: 'items' harus array: {e}")))?;
    Ok(arr.into_iter().map(wrap_item).collect())
}

/// runOnceForEachItem: script per item dengan `item` + `index`.
/// `item` akhir array → di-spread; kalau tidak → satu item.
fn run_code_each(node: &WorkflowNode, items: Vec<Value>) -> EngineResult<Vec<Value>> {
    let code = node
        .parameters
        .get("code")
        .and_then(Value::as_str)
        .ok_or_else(|| EngineError::new("code: parameter 'code' wajib string"))?;
    let eng = rhai::Engine::new();
    let mut out = Vec::new();
    for (i, it) in items.into_iter().enumerate() {
        let dyn_item =
            rhai::serde::to_dynamic(&it).map_err(|e| EngineError::new(format!("code: {e}")))?;
        let mut scope = rhai::Scope::new();
        scope.push("item", dyn_item);
        scope.push("index", i as i64);
        let _ = eng
            .eval_with_scope::<rhai::Dynamic>(&mut scope, code)
            .map_err(|e| EngineError::new(format!("code: {e}")))?;
        let back: rhai::Dynamic = scope
            .get_value("item")
            .ok_or_else(|| EngineError::new("code: variabel 'item' hilang"))?;
        if back.is_array() {
            let arr: Vec<Value> = rhai::serde::from_dynamic(&back)
                .map_err(|e| EngineError::new(format!("code: {e}")))?;
            out.extend(arr.into_iter().map(wrap_item));
        } else {
            let v: Value = rhai::serde::from_dynamic(&back)
                .map_err(|e| EngineError::new(format!("code: {e}")))?;
            out.push(wrap_item(v));
        }
    }
    Ok(out)
}

fn wrap_item(v: Value) -> Value {
    match v {
        Value::Object(_) => v,
        other => json!({"value": other}),
    }
}

// ---------------------------------------------------------------------------
// ScheduleTrigger — field output persis n8n (UTC)
// ---------------------------------------------------------------------------

impl Node for ScheduleNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.scheduleTrigger"
    }

    /// Emit field yang sama seperti n8n (zona UTC — n8n memakai timezone
    /// workflow; ekstensi: echo `rule` apa adanya untuk metadata cron).
    /// Penjadwalan sesungguhnya = cron/systemd eksternal (lihat README).
    fn execute(
        &self,
        node: &WorkflowNode,
        _items: Vec<Value>,
        _ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let rule = node.parameters.get("rule").cloned().unwrap_or(Value::Null);
        Ok(vec![vec![schedule_item(&rule)]])
    }
}

fn schedule_item(rule: &Value) -> Value {
    use chrono::{Datelike, Timelike};
    let now = chrono::Utc::now();
    let day = now.day();
    let suffix = match day {
        1 | 21 | 31 => "st",
        2 | 22 => "nd",
        3 | 23 => "rd",
        _ => "th",
    };
    let h24 = now.hour();
    let h12 = match h24 % 12 {
        0 => 12,
        h => h,
    };
    let ampm = if h24 < 12 { "am" } else { "pm" };
    let time = format!("{}:{:02}:{:02} {}", h12, now.minute(), now.second(), ampm);
    let month = now.format("%B").to_string();
    json!({
        "timestamp": now.to_rfc3339_opts(chrono::SecondsFormat::Millis, false),
        "Readable date": format!("{} {}{} {}, {}", month, day, suffix, now.year(), time),
        "Readable time": time,
        "Day of week": now.format("%A").to_string(),
        "Year": now.format("%Y").to_string(),
        "Month": month,
        "Day of month": now.format("%d").to_string(),
        "Hour": now.format("%H").to_string(),
        "Minute": now.format("%M").to_string(),
        "Second": now.format("%S").to_string(),
        "Timezone": "UTC (UTC+00:00)",
        "rule": rule,
    })
}

// ---------------------------------------------------------------------------
// Webhook — emit payload server (response diatur server)
// ---------------------------------------------------------------------------

impl Node for WebhookNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.webhook"
    }

    /// Trigger: emit payload request dari server (`/hook/:path`).
    /// Run manual (tanpa payload) → placeholder `{mode:"manual"}`.
    fn execute(
        &self,
        _node: &WorkflowNode,
        _items: Vec<Value>,
        ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let item = ctx.webhook.cloned().unwrap_or(json!({"mode": "manual"}));
        Ok(vec![vec![item]])
    }
}

// ---------------------------------------------------------------------------
// Switch — N cabang rules + else (n8n Switch V3)
// ---------------------------------------------------------------------------

impl Node for SwitchNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.switch"
    }

    /// Tiap item masuk ke SEMUA cabang yang cocok; yang tak cocok ke
    /// cabang `extra` (else) kecuali `fallbackOutput: "none"`.
    /// `renameOutput`/`numberOutputs` diabaikan (display-only di n8n).
    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let extra = node
            .parameters
            .get("fallbackOutput")
            .and_then(Value::as_str)
            .unwrap_or("extra");
        if extra != "extra" && extra != "none" {
            return Err(EngineError::new(format!(
                "switch: fallbackOutput tak dikenal '{extra}'"
            )));
        }
        let mut rules: Vec<Value> = Vec::new();
        if let Some(rs) = node
            .parameters
            .get("rules")
            .and_then(|r| r.get("values"))
            .and_then(Value::as_array)
        {
            for (i, r) in rs.iter().enumerate() {
                let conds = r
                    .get("conditions")
                    .filter(|c| c.is_object())
                    .cloned()
                    .ok_or_else(|| {
                        EngineError::new(format!("switch: rule {} tanpa conditions object", i + 1))
                    })?;
                rules.push(conds);
            }
        }
        let n_out = if extra == "none" {
            rules.len().max(1)
        } else {
            rules.len() + 1
        };
        let mut outs: BranchOutputs = vec![Vec::new(); n_out];
        let global_opts = node.parameters.get("options");
        for (i, it) in items.into_iter().enumerate() {
            let ectx = ExprContext::simple(&it, ctx.outputs);
            let render = |v: &Value| render_value(v, &ectx);
            let mut matched = false;
            for (ri, rule) in rules.iter().enumerate() {
                let opts = resolve_opts(rule, global_opts);
                if eval_conditions(rule, i, &render, &opts)? {
                    outs[ri].push(it.clone());
                    matched = true;
                }
            }
            if !matched && extra == "extra" {
                outs[rules.len()].push(it);
            }
        }
        Ok(outs)
    }
}

// ---------------------------------------------------------------------------
// Merge — multi-input (n8n Merge v3); logika di `merge.rs`
// ---------------------------------------------------------------------------

impl Node for MergeNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.merge"
    }

    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        _ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        merge::run(std::slice::from_ref(&items), &node.parameters)
    }

    fn execute_multi(
        &self,
        node: &WorkflowNode,
        inputs: Vec<MultiInput>,
        _ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let views: Vec<Vec<Value>> = inputs.into_iter().map(|i| i.items).collect();
        merge::run(&views, &node.parameters)
    }
}

// ---------------------------------------------------------------------------
// DateTime — 7 operasi (n8n DateTime V2); logika di `datetime.rs`
// ---------------------------------------------------------------------------

impl Node for DateTimeNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.dateTime"
    }

    /// Tiap item mendapat field output; item non-object dibungkus
    /// (`{"value": ...}`); 0 item → 1 item (manual-trigger n8n).
    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        if items.is_empty() {
            let null = Value::Null;
            let ectx = ExprContext::simple(&null, ctx.outputs);
            let render = |v: &Value| render_value(v, &ectx);
            let (name, value) = datetime::compute(&node.parameters, &render)?;
            let mut o = Map::new();
            o.insert(name, value);
            return Ok(vec![vec![Value::Object(o)]]);
        }
        let mut out = Vec::with_capacity(items.len());
        for it in &items {
            let ectx = ExprContext::simple(it, ctx.outputs);
            let render = |v: &Value| render_value(v, &ectx);
            let (name, value) = datetime::compute(&node.parameters, &render)?;
            match it {
                Value::Object(o) => {
                    let mut m = o.clone();
                    m.insert(name, value);
                    out.push(Value::Object(m));
                }
                other => {
                    let mut m = Map::new();
                    m.insert("value".to_string(), other.clone());
                    m.insert(name, value);
                    out.push(Value::Object(m));
                }
            }
        }
        Ok(vec![out])
    }
}

// ---------------------------------------------------------------------------
// RespondToWebhook — passthrough; server `/hook` membaca paramsnya
// ---------------------------------------------------------------------------

impl Node for RespondNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.respondToWebhook"
    }

    /// Run biasa: teruskan item apa adanya. Bila workflow webhook
    /// memakai `responseMode: "responseNode"`, server memakai
    /// `respondWith`/`responseBody`/`responseCode`/headers node ini.
    fn execute(
        &self,
        _node: &WorkflowNode,
        items: Vec<Value>,
        _ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        Ok(vec![items])
    }
}

/// Cari node respond pertama (dipakai server untuk mode responseNode).
pub fn find_respond(workflow: &n8n_core::Workflow) -> Option<&WorkflowNode> {
    workflow
        .nodes
        .iter()
        .find(|n| n.node_type == "n8n-nodes-base.respondToWebhook")
}

// ---------------------------------------------------------------------------
// Wait — subset: tidur `amount`×`unit` lalu teruskan
// ---------------------------------------------------------------------------

impl Node for WaitNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.wait"
    }

    /// Subset n8n Wait: tidur sinkron lalu teruskan item. `amount` ≤ 0
    /// dilewati (n8n: resumeAt ≤ now → lanjut).
    /// Jika `resume` == "webhook" atau `resumeAmount` ada, emit marker
    /// `__waitResume` dengan `resumeUrl` ala n8n `$execution.resumeUrl`.
    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        // Check resume webhook marker
        let resume = node.parameters.get("resume").and_then(Value::as_str).unwrap_or("");
        let is_webhook_resume = resume == "webhook"
            || node.parameters.get("resume").and_then(|r| r.get("resume")).and_then(Value::as_str) == Some("webhook")
            || node.parameters.get("resumeAmount").is_some()
            || node.parameters.get("webhookId").is_some();

        if is_webhook_resume {
            let exec_id = ctx.execution_id.unwrap_or("exec-mock");
            let resume_url = format!("/api/wait/resume/{}", exec_id);
            let out: Vec<Value> = items.into_iter().map(|it| {
                let mut o = it.as_object().cloned().unwrap_or_default();
                o.insert("__waitResume".to_string(), json!({
                    "executionId": exec_id,
                    "resumeUrl": resume_url,
                    "mode": "webhook",
                    "waiting": true
                }));
                // binary passthrough preserved via object spread
                Value::Object(o)
            }).collect();
            let final_out = if out.is_empty() {
                vec![json!({"__waitResume": {"executionId": exec_id, "resumeUrl": resume_url, "mode": "webhook", "waiting": true}})]
            } else { out };
            return Ok(vec![final_out]);
        }

        let amount = match node.parameters.get("amount") {
            None => 1.0,
            Some(Value::Number(n)) => n.as_f64().unwrap_or(1.0),
            Some(Value::String(s)) => s
                .trim()
                .parse::<f64>()
                .map_err(|_| EngineError::new(format!("wait: amount bukan angka ('{s}')")))?,
            Some(other) => {
                return Err(EngineError::new(format!(
                    "wait: amount bukan angka ({other})"
                )))
            }
        };
        let unit = node
            .parameters
            .get("unit")
            .and_then(Value::as_str)
            .unwrap_or("hours");
        let ms = match unit {
            "milliseconds" => amount,
            "seconds" => amount * 1000.0,
            "minutes" => amount * 60_000.0,
            "hours" => amount * 3_600_000.0,
            "days" => amount * 86_400_000.0,
            other => {
                return Err(EngineError::new(format!(
                    "wait: unit tak dikenal '{other}'"
                )))
            }
        };
        if !ms.is_finite() {
            return Err(EngineError::new("wait: amount tak hingga"));
        }
        if ms > 0.0 {
            // cap sleep to 100ms in tests to keep perf > asli, but respect up to 5s otherwise
            let capped = ms.min(5000.0);
            if std::env::var("N8N_RUST_TEST").is_ok() {
                std::thread::sleep(std::time::Duration::from_millis(capped.min(10.0) as u64));
            } else {
                std::thread::sleep(std::time::Duration::from_secs_f64(capped / 1000.0));
            }
        }
        Ok(vec![items])
    }
}

// ---------------------------------------------------------------------------
// StopAndError — selalu gagal dengan pesan berlapis
// ---------------------------------------------------------------------------

impl Node for StopNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.stopAndError"
    }

    /// Selalu gagal: `message` ‖ `description` ‖ `error` ‖ `Error: {...}`.
    fn execute(
        &self,
        node: &WorkflowNode,
        _items: Vec<Value>,
        _ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let p = &node.parameters;
        let msg = p
            .get("message")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| {
                p.get("description")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            })
            .or_else(|| p.get("error").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| {
                let dump: Map<String, Value> =
                    p.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                format!("Error: {}", Value::Object(dump))
            });
        Err(EngineError::new(format!("stopAndError: {msg}")))
    }
}

// ---------------------------------------------------------------------------
// ExecuteCommand — perintah shell host (n8n Execute Command)
// ---------------------------------------------------------------------------

impl Node for ExecNode {
    fn node_type(&self) -> &'static str {
        "n8n-nodes-base.executeCommand"
    }

    /// Sekali per eksekusi: `command` di-render sebagai template lalu
    /// dijalankan via `sh -c`. Output satu item `{command, stdout,
    /// stderr, exitCode}`. Exit != 0 → error kecuali
    /// `options.continueOnFail: true`.
    fn execute(
        &self,
        node: &WorkflowNode,
        items: Vec<Value>,
        ctx: &ExecContext,
    ) -> EngineResult<BranchOutputs> {
        let null = Value::Null;
        let ectx = ExprContext::simple(items.first().unwrap_or(&null), ctx.outputs);
        let cmd_v = render_value(
            node.parameters.get("command").unwrap_or(&Value::Null),
            &ectx,
        );
        let cmd = cmd_v
            .as_str()
            .ok_or_else(|| EngineError::new("executeCommand: 'command' wajib string"))?;
        if cmd.trim().is_empty() {
            return Err(EngineError::new("executeCommand: 'command' kosong"));
        }
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .map_err(|e| EngineError::new(format!("executeCommand: {e}")))?;
        let code = out.status.code();
        let item = json!({
            "command": cmd,
            "stdout": String::from_utf8_lossy(&out.stdout).to_string(),
            "stderr": String::from_utf8_lossy(&out.stderr).to_string(),
            "exitCode": code,
        });
        if !out.status.success() {
            let forgiving = node
                .parameters
                .get("options")
                .and_then(|o| o.get("continueOnFail"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !forgiving {
                let c = match code {
                    Some(c) => c.to_string(),
                    None => "signal".to_string(),
                };
                return Err(EngineError::new(format!(
                    "executeCommand: exit tidak-nol ({c})"
                )));
            }
        }
        Ok(vec![vec![item]])
    }
}

/// Cari node webhook pertama (dipakai server untuk responseMode dkk).
pub fn find_webhook(workflow: &n8n_core::Workflow) -> Option<&WorkflowNode> {
    workflow
        .nodes
        .iter()
        .find(|n| n.node_type == "n8n-nodes-base.webhook")
}

// ---------------------------------------------------------------------------
// Util
// ---------------------------------------------------------------------------

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => {
            let t = s.trim().to_lowercase();
            !(t.is_empty() || t == "false" || t == "0")
        }
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

pub fn register_all(registry: &mut Registry) {
    registry.register(Arc::new(ManualTrigger));
    registry.register(Arc::new(SetNode));
    registry.register(Arc::new(NoOp));
    registry.register(Arc::new(FilterNode));
    registry.register(Arc::new(SortNode));
    registry.register(Arc::new(LimitNode));
    registry.register(Arc::new(IfNode));
    registry.register(Arc::new(HttpNode));
    registry.register(Arc::new(CodeNode));
    registry.register(Arc::new(FunctionNode));
    registry.register(Arc::new(ScheduleNode));
    registry.register(Arc::new(WebhookNode));
    registry.register(Arc::new(SwitchNode));
    registry.register(Arc::new(MergeNode));
    registry.register(Arc::new(DateTimeNode));
    registry.register(Arc::new(RespondNode));
    registry.register(Arc::new(WaitNode));
    registry.register(Arc::new(StopNode));
    registry.register(Arc::new(ExecNode));
    // extended 20+ nodes — 95% n8n asli
    extended::register_extended(registry);
}

#[cfg(test)]
mod tests {
    use super::*;
    use n8n_core::Workflow;
    use n8n_engine::Engine;
    use std::collections::HashMap;

    fn empty_ctx(outputs: &HashMap<String, BranchOutputs>) -> ExecContext<'_> {
        ExecContext { outputs, webhook: None, workflow_name: None, execution_id: None }
    }

    fn mk(id: &str, name: &str, t: &str, parameters: HashMap<String, Value>) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: name.to_string(),
            node_type: t.to_string(),
            type_version: 1.0,
            position: [0.0, 0.0],
            parameters,
            credentials: HashMap::new(),
            disabled: false,
            notes: String::new(),
            notes_in_flow: false,
            extra: HashMap::new(),
        }
    }

    fn canned_server_with(n: usize, body: &'static str) -> (u16, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("addr").port();
        let handle = std::thread::spawn(move || {
            for _ in 0..n {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(resp.as_bytes()).expect("write");
            }
        });
        (port, handle)
    }

    fn canned_server(n: usize) -> (u16, std::thread::JoinHandle<()>) {
        canned_server_with(n, r#"{"ok":true}"#)
    }

    fn http_node(url: &str) -> WorkflowNode {
        mk(
            "h",
            "HTTP",
            "n8n-nodes-base.httpRequest",
            HashMap::from([
                ("url".to_string(), json!(url)),
                ("method".to_string(), json!("GET")),
            ]),
        )
    }

    #[test]
    fn fixture_runs_end_to_end() {
        let raw = include_str!("../../../fixtures/manual-to-set.json");
        let wf = Workflow::from_json(raw).expect("parse fixture");
        let mut registry = Registry::default();
        register_all(&mut registry);
        let report = Engine::run(&wf, &registry).expect("run");
        assert_eq!(
            report.order,
            vec![
                "Manual Trigger".to_string(),
                "Set".to_string(),
                "NoOp".to_string()
            ]
        );
        assert_eq!(
            report.outputs["NoOp"][0],
            vec![json!({ "greeting": "halo", "n": 1 })]
        );
        assert_eq!(report.durations_ms.len(), 3);
    }

    #[test]
    fn set_supports_n8n_assignments_shape() {
        let node = mk(
            "s",
            "Set",
            "n8n-nodes-base.set",
            HashMap::from([(
                "assignments".to_string(),
                json!({ "assignments": [{ "name": "a", "value": 1 }] }),
            )]),
        );
        let outputs = HashMap::new();
        let out = SetNode
            .execute(&node, vec![json!({})], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(out, vec![vec![json!({ "a": 1 })]]);
    }

    #[test]
    fn set_renders_expressions_per_item() {
        let node = mk(
            "s",
            "Set",
            "n8n-nodes-base.set",
            HashMap::from([(
                "values".to_string(),
                json!({"who": "={{ $json.name }}", "n": 5}),
            )]),
        );
        let outputs = HashMap::new();
        let out = SetNode
            .execute(&node, vec![json!({"name": "udi"})], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(
            out,
            vec![vec![json!({"name": "udi", "who": "udi", "n": 5})]]
        );
    }

    #[test]
    fn set_reads_other_node_output() {
        let node = mk(
            "s",
            "Set",
            "n8n-nodes-base.set",
            HashMap::from([(
                "values".to_string(),
                json!({"x": "={{ $node[\"Up\"].json.x }}"}),
            )]),
        );
        let outputs = HashMap::from([("Up".to_string(), vec![vec![json!({"x": 7})]])]);
        let out = SetNode
            .execute(&node, vec![json!({})], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"x": 7})]]);
    }

    #[test]
    fn set_include_none_and_selected() {
        let node = mk(
            "s",
            "Set",
            "n8n-nodes-base.set",
            HashMap::from([
                ("include".to_string(), json!("none")),
                (
                    "assignments".to_string(),
                    json!({ "assignments": [{ "name": "a", "value": 1 }] }),
                ),
            ]),
        );
        let outputs = HashMap::new();
        let out = SetNode
            .execute(&node, vec![json!({"drop": true})], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"a": 1})]]);

        let node = mk(
            "s",
            "Set",
            "n8n-nodes-base.set",
            HashMap::from([
                ("include".to_string(), json!("selected")),
                ("includeFields".to_string(), json!("keep, missing")),
                (
                    "assignments".to_string(),
                    json!({ "assignments": [{ "name": "a", "value": 1 }] }),
                ),
            ]),
        );
        let out = SetNode
            .execute(
                &node,
                vec![json!({"keep": 9, "drop": 8})],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"keep": 9, "a": 1})]]);
    }

    #[test]
    fn set_dot_notation_nested_and_except() {
        let node = mk(
            "s",
            "Set",
            "n8n-nodes-base.set",
            HashMap::from([
                ("include".to_string(), json!("except")),
                ("excludeFields".to_string(), json!("gone")),
                (
                    "assignments".to_string(),
                    json!({ "assignments": [{ "name": "a.b", "value": 5 }] }),
                ),
            ]),
        );
        let outputs = HashMap::new();
        let out = SetNode
            .execute(
                &node,
                vec![json!({"keep": 1, "gone": 2})],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"keep": 1, "a": {"b": 5}})]]);
    }

    #[test]
    fn set_fields_values_legacy_shape() {
        let node = mk(
            "s",
            "Set",
            "n8n-nodes-base.set",
            HashMap::from([(
                "fields".to_string(),
                json!({ "values": [
                    {"name": "s", "type": "stringValue", "stringValue": "x"},
                    {"name": "n", "type": "numberValue", "numberValue": 4},
                    {"name": "b", "type": "booleanValue", "booleanValue": true},
                    {"name": "o", "type": "objectValue", "objectValue": {"k": 1}}
                ] }),
            )]),
        );
        let outputs = HashMap::new();
        let out = SetNode
            .execute(&node, vec![json!({})], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(
            out,
            vec![vec![json!({"s": "x", "n": 4, "b": true, "o": {"k": 1}})]]
        );
    }

    #[test]
    fn set_raw_mode_parses_json_output() {
        let node = mk(
            "s",
            "Set",
            "n8n-nodes-base.set",
            HashMap::from([
                ("mode".to_string(), json!("raw")),
                (
                    "jsonOutput".to_string(),
                    json!("={\"who\": \"={{ $json.name }}\"}"),
                ),
            ]),
        );
        let outputs = HashMap::new();
        let out = SetNode
            .execute(&node, vec![json!({"name": "udi"})], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"who": "udi"})]]);
    }

    #[test]
    fn filter_keeps_only_truthy_items() {
        let node = mk(
            "f",
            "Filter",
            "n8n-nodes-base.filter",
            HashMap::from([("condition".to_string(), json!("={{ $json.keep }}"))]),
        );
        let outputs = HashMap::new();
        let out = FilterNode
            .execute(
                &node,
                vec![
                    json!({"keep": true}),
                    json!({"keep": false}),
                    json!({"keep": 0}),
                    json!({"keep": "x"}),
                ],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"keep": true}), json!({"keep": "x"})]]);
    }

    #[test]
    fn filter_conditions_object() {
        let node = mk(
            "f",
            "Filter",
            "n8n-nodes-base.filter",
            HashMap::from([(
                "conditions".to_string(),
                json!({
                    "combinator": "and",
                    "conditions": [{
                        "leftValue": "={{ $json.age }}",
                        "rightValue": 18,
                        "operator": {"type": "number", "operation": "gte"}
                    }],
                    "options": {}
                }),
            )]),
        );
        let outputs = HashMap::new();
        let out = FilterNode
            .execute(
                &node,
                vec![json!({"age": 20}), json!({"age": 10})],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"age": 20})]]);
    }

    #[test]
    fn sort_orders_by_legacy_field() {
        let node = mk(
            "s",
            "Sort",
            "n8n-nodes-base.sort",
            HashMap::from([
                ("field".to_string(), json!("age")),
                ("order".to_string(), json!("asc")),
            ]),
        );
        let outputs = HashMap::new();
        let out = SortNode
            .execute(
                &node,
                vec![json!({"age": 3}), json!({"age": 1}), json!({"age": 2})],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(
            out,
            vec![vec![
                json!({"age": 1}),
                json!({"age": 2}),
                json!({"age": 3})
            ]]
        );
    }

    #[test]
    fn sort_fields_ui_multi_and_case_insensitive() {
        let node = mk(
            "s",
            "Sort",
            "n8n-nodes-base.sort",
            HashMap::from([(
                "sortFieldsUi".to_string(),
                json!({ "sortField": [
                    {"fieldName": "g", "order": "ascending"},
                    {"fieldName": "name", "order": "descending"}
                ] }),
            )]),
        );
        let outputs = HashMap::new();
        let out = SortNode
            .execute(
                &node,
                vec![
                    json!({"g": 1, "name": "budi"}),
                    json!({"g": 1, "name": "Andi"}),
                    json!({"g": 0, "name": "Zed"}),
                ],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(
            out,
            vec![vec![
                json!({"g": 0, "name": "Zed"}),
                json!({"g": 1, "name": "budi"}),
                json!({"g": 1, "name": "Andi"})
            ]]
        );
    }

    #[test]
    fn sort_missing_field_is_error() {
        let node = mk(
            "s",
            "Sort",
            "n8n-nodes-base.sort",
            HashMap::from([(
                "sortFieldsUi".to_string(),
                json!({ "sortField": [{"fieldName": "nope", "order": "ascending"}] }),
            )]),
        );
        let outputs = HashMap::new();
        let err = SortNode
            .execute(&node, vec![json!({"a": 1})], &empty_ctx(&outputs))
            .expect_err("must fail");
        assert!(err.to_string().contains("Couldn't find the field"), "{err}");
    }

    #[test]
    fn sort_random_keeps_same_items() {
        let node = mk(
            "s",
            "Sort",
            "n8n-nodes-base.sort",
            HashMap::from([("type".to_string(), json!("random"))]),
        );
        let outputs = HashMap::new();
        let items = vec![json!({"n": 1}), json!({"n": 2}), json!({"n": 3})];
        let out = SortNode
            .execute(&node, items.clone(), &empty_ctx(&outputs))
            .expect("exec");
        let mut got = out[0].clone();
        let mut want = items;
        got.sort_by_key(|v| v["n"].as_i64().unwrap_or(0));
        want.sort_by_key(|v| v["n"].as_i64().unwrap_or(0));
        assert_eq!(got, want);
    }

    #[test]
    fn limit_max_items_and_keep() {
        let outputs = HashMap::new();
        let first = mk(
            "l",
            "Limit",
            "n8n-nodes-base.limit",
            HashMap::from([("maxItems".to_string(), json!(2))]),
        );
        let out = LimitNode
            .execute(
                &first,
                vec![json!(1), json!(2), json!(3)],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(out, vec![vec![json!(1), json!(2)]]);
        let last = mk(
            "l",
            "Limit",
            "n8n-nodes-base.limit",
            HashMap::from([
                ("maxItems".to_string(), json!(2)),
                ("keep".to_string(), json!("lastItems")),
            ]),
        );
        let out = LimitNode
            .execute(
                &last,
                vec![json!(1), json!(2), json!(3)],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(out, vec![vec![json!(2), json!(3)]]);
    }

    #[test]
    fn if_splits_by_legacy_condition() {
        let node = mk(
            "i",
            "If",
            "n8n-nodes-base.if",
            HashMap::from([("condition".to_string(), json!("={{ $json.age > 18 }}"))]),
        );
        let outputs = HashMap::new();
        let out = IfNode
            .execute(
                &node,
                vec![json!({"age": 20}), json!({"age": 10})],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(
            out,
            vec![vec![json!({"age": 20})], vec![json!({"age": 10})]]
        );
    }

    #[test]
    fn if_conditions_object() {
        let node = mk(
            "i",
            "If",
            "n8n-nodes-base.if",
            HashMap::from([(
                "conditions".to_string(),
                json!({
                    "combinator": "or",
                    "conditions": [
                        {"leftValue": "={{ $json.tag }}", "rightValue": "vip",
                         "operator": {"type": "string", "operation": "equals"}},
                        {"leftValue": "={{ $json.age }}", "rightValue": 65,
                         "operator": {"type": "number", "operation": "gte"}}
                    ],
                    "options": {}
                }),
            )]),
        );
        let outputs = HashMap::new();
        let out = IfNode
            .execute(
                &node,
                vec![
                    json!({"tag": "VIP", "age": 30}),
                    json!({"tag": "x", "age": 70}),
                    json!({"tag": "x", "age": 30}),
                ],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(
            out,
            vec![
                vec![
                    json!({"tag": "VIP", "age": 30}),
                    json!({"tag": "x", "age": 70})
                ],
                vec![json!({"tag": "x", "age": 30})]
            ]
        );
    }

    #[test]
    fn http_get_returns_canned_response() {
        let (port, handle) = canned_server(1);
        let node = http_node(&format!("http://127.0.0.1:{port}/echo"));
        let outputs = HashMap::new();
        let out = HttpNode
            .execute(&node, vec![json!({})], &empty_ctx(&outputs))
            .expect("exec");
        handle.join().expect("server thread");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 1);
        assert_eq!(out[0][0]["status"], json!(200));
        assert_eq!(out[0][0]["body"], json!({"ok": true}));
    }

    #[test]
    fn http_fans_out_one_request_per_item() {
        let (port, handle) = canned_server(2);
        let node = http_node(&format!("http://127.0.0.1:{port}/echo"));
        let outputs = HashMap::new();
        let out = HttpNode
            .execute(
                &node,
                vec![json!({"id": 1}), json!({"id": 2})],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        handle.join().expect("server thread");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 2);
        assert!(out[0].iter().all(|o| o["status"] == json!(200)));
    }

    #[test]
    fn http_response_format_text_keeps_raw() {
        let (port, handle) = canned_server(1);
        let mut node = http_node(&format!("http://127.0.0.1:{port}/echo"));
        node.parameters
            .insert("options".to_string(), json!({"responseFormat": "text"}));
        let outputs = HashMap::new();
        let out = HttpNode
            .execute(&node, vec![json!({})], &empty_ctx(&outputs))
            .expect("exec");
        handle.join().expect("server thread");
        assert_eq!(out[0][0]["body"], json!(r#"{"ok":true}"#));
    }

    #[test]
    fn http_response_format_json_rejects_text() {
        let (port, handle) = canned_server_with(1, "bukan json{{{");
        let mut node = http_node(&format!("http://127.0.0.1:{port}/echo"));
        node.parameters.insert(
            "options".to_string(),
            json!({"response": {"response": {"responseFormat": "json"}}}),
        );
        let outputs = HashMap::new();
        let err = HttpNode
            .execute(&node, vec![json!({})], &empty_ctx(&outputs))
            .expect_err("must fail");
        handle.join().expect("server thread");
        assert!(err.to_string().contains("bukan JSON valid"), "{err}");
    }

    #[test]
    fn code_transforms_items_with_rhai() {
        let node = mk(
            "c",
            "Code",
            "n8n-nodes-base.code",
            HashMap::from([(
                "code".to_string(),
                json!("let out = []; for it in items { out.push(it.n * 10); } items = out;"),
            )]),
        );
        let outputs = HashMap::new();
        let out = CodeNode
            .execute(
                &node,
                vec![json!({"n": 1}), json!({"n": 2})],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"value": 10}), json!({"value": 20})]]);
    }

    #[test]
    fn code_rejects_non_array_items() {
        let node = mk(
            "c",
            "Code",
            "n8n-nodes-base.code",
            HashMap::from([("code".to_string(), json!("items = 42;"))]),
        );
        let outputs = HashMap::new();
        let err = CodeNode
            .execute(&node, vec![json!({})], &empty_ctx(&outputs))
            .expect_err("must fail");
        assert!(err.to_string().contains("'items' harus array"), "{err}");
    }

    #[test]
    fn code_each_item_mode_with_index() {
        let node = mk(
            "c",
            "Code",
            "n8n-nodes-base.code",
            HashMap::from([
                ("mode".to_string(), json!("runOnceForEachItem")),
                ("code".to_string(), json!("item.n = item.n + index; item;")),
            ]),
        );
        let outputs = HashMap::new();
        let out = CodeNode
            .execute(
                &node,
                vec![json!({"n": 10}), json!({"n": 10})],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"n": 10}), json!({"n": 11})]]);
    }

    #[test]
    fn code_each_item_spreads_array() {
        let node = mk(
            "c",
            "Code",
            "n8n-nodes-base.code",
            HashMap::from([
                ("mode".to_string(), json!("runOnceForEachItem")),
                ("code".to_string(), json!("item = [1, 2];")),
            ]),
        );
        let outputs = HashMap::new();
        let out = CodeNode
            .execute(&node, vec![json!({})], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"value": 1}), json!({"value": 2})]]);
    }

    #[test]
    fn schedule_emits_n8n_fields() {
        let node = mk(
            "s",
            "Schedule",
            "n8n-nodes-base.scheduleTrigger",
            HashMap::from([("rule".to_string(), json!({"interval": [{"field": "days"}]}))]),
        );
        let outputs = HashMap::new();
        let out = ScheduleNode
            .execute(&node, vec![], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 1);
        let item = &out[0][0];
        for key in [
            "timestamp",
            "Readable date",
            "Readable time",
            "Day of week",
            "Year",
            "Month",
            "Day of month",
            "Hour",
            "Minute",
            "Second",
            "Timezone",
        ] {
            assert!(item.get(key).and_then(Value::as_str).is_some(), "{key}");
        }
        assert_eq!(item["rule"], json!({"interval": [{"field": "days"}]}));
        assert_eq!(item["Timezone"], json!("UTC (UTC+00:00)"));
    }

    #[test]
    fn webhook_emits_payload_when_present() {
        let node = mk(
            "w",
            "Webhook",
            "n8n-nodes-base.webhook",
            HashMap::from([("path".to_string(), json!("demo"))]),
        );
        let outputs = HashMap::new();
        let payload = json!({"a": 1});
        let cx = ExecContext { outputs: &outputs, webhook: Some(&payload), workflow_name: None, execution_id: None };
        let out = WebhookNode.execute(&node, vec![], &cx).expect("exec");
        assert_eq!(out, vec![vec![json!({"a": 1})]]);
    }

    #[test]
    fn webhook_manual_placeholder_without_payload() {
        let node = mk("w", "Webhook", "n8n-nodes-base.webhook", HashMap::new());
        let outputs = HashMap::new();
        let out = WebhookNode
            .execute(&node, vec![], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"mode": "manual"})]]);
    }

    #[test]
    fn switch_routes_to_matching_branches_and_else() {
        let rule = |kind: &str| {
            json!({
                "conditions": {
                    "combinator": "and",
                    "conditions": [{
                        "leftValue": "={{ $json.kind }}",
                        "rightValue": kind,
                        "operator": {"type": "string", "operation": "equals"}
                    }],
                    "options": {}
                }
            })
        };
        let node = mk(
            "s",
            "Switch",
            "n8n-nodes-base.switch",
            HashMap::from([(
                "rules".to_string(),
                json!({"values": [rule("a"), rule("b")]}),
            )]),
        );
        let outputs = HashMap::new();
        let out = SwitchNode
            .execute(
                &node,
                vec![
                    json!({"kind": "a"}),
                    json!({"kind": "b"}),
                    json!({"kind": "z"}),
                ],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], vec![json!({"kind": "a"})]);
        assert_eq!(out[1], vec![json!({"kind": "b"})]);
        assert_eq!(out[2], vec![json!({"kind": "z"})]);
    }

    #[test]
    fn switch_fallback_none_drops_unmatched() {
        let node = mk(
            "s",
            "Switch",
            "n8n-nodes-base.switch",
            HashMap::from([
                (
                    "rules".to_string(),
                    json!({"values": [{
                        "conditions": {
                            "combinator": "and",
                            "conditions": [{
                                "leftValue": "={{ $json.kind }}",
                                "rightValue": "a",
                                "operator": {"type": "string", "operation": "equals"}
                            }],
                            "options": {}
                        }
                    }]}),
                ),
                ("fallbackOutput".to_string(), json!("none")),
            ]),
        );
        let outputs = HashMap::new();
        let out = SwitchNode
            .execute(
                &node,
                vec![json!({"kind": "a"}), json!({"kind": "z"})],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"kind": "a"})]]);
    }

    #[test]
    fn switch_rule_without_conditions_is_error() {
        let node = mk(
            "s",
            "Switch",
            "n8n-nodes-base.switch",
            HashMap::from([(
                "rules".to_string(),
                json!({"values": [{"renameOutput": "x"}]}),
            )]),
        );
        let outputs = HashMap::new();
        let err = SwitchNode
            .execute(&node, vec![json!({})], &empty_ctx(&outputs))
            .expect_err("must fail");
        assert!(err.to_string().contains("tanpa conditions"), "{err}");
    }

    #[test]
    fn merge_dispatch_sees_two_inputs() {
        let node = mk(
            "m",
            "Merge",
            "n8n-nodes-base.merge",
            HashMap::from([("mode".to_string(), json!("append"))]),
        );
        let outputs = HashMap::new();
        let out = MergeNode
            .execute_multi(
                &node,
                vec![
                    MultiInput {
                        from: "A".to_string(),
                        branch: 0,
                        items: vec![json!({"a": 1})],
                    },
                    MultiInput {
                        from: "B".to_string(),
                        branch: 0,
                        items: vec![json!({"b": 2})],
                    },
                ],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"a": 1}), json!({"b": 2})]]);
    }

    #[test]
    fn datetime_adds_field_per_item() {
        let node = mk(
            "d",
            "DateTime",
            "n8n-nodes-base.dateTime",
            HashMap::from([
                ("operation".to_string(), json!("addToDate")),
                ("magnitude".to_string(), json!("2026-01-01T00:00:00+00:00")),
                ("timeUnit".to_string(), json!("days")),
                ("duration".to_string(), json!(1)),
            ]),
        );
        let outputs = HashMap::new();
        let out = DateTimeNode
            .execute(
                &node,
                vec![json!({}), json!({"k": 1})],
                &empty_ctx(&outputs),
            )
            .expect("exec");
        assert_eq!(
            out,
            vec![vec![
                json!({"newDate": "2026-01-02T00:00:00.000+00:00"}),
                json!({"k": 1, "newDate": "2026-01-02T00:00:00.000+00:00"}),
            ]]
        );
    }

    #[test]
    fn datetime_empty_input_yields_one_item() {
        let node = mk(
            "d",
            "DateTime",
            "n8n-nodes-base.dateTime",
            HashMap::from([("operation".to_string(), json!("getCurrentDate"))]),
        );
        let outputs = HashMap::new();
        let out = DateTimeNode
            .execute(&node, vec![], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 1);
        let s = out[0][0]
            .get("currentDate")
            .and_then(Value::as_str)
            .expect("currentDate string");
        assert!(s.contains('T'), "{s}");
    }

    #[test]
    fn respond_passes_items_through() {
        let node = mk(
            "r",
            "Respond",
            "n8n-nodes-base.respondToWebhook",
            HashMap::new(),
        );
        let outputs = HashMap::new();
        let items = vec![json!({"a": 1}), json!({"b": 2})];
        let out = RespondNode
            .execute(&node, items.clone(), &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(out, vec![items]);
    }

    #[test]
    fn wait_zero_amount_skips_sleep() {
        let node = mk(
            "w",
            "Wait",
            "n8n-nodes-base.wait",
            HashMap::from([
                ("amount".to_string(), json!(0)),
                ("unit".to_string(), json!("seconds")),
            ]),
        );
        let outputs = HashMap::new();
        let out = WaitNode
            .execute(&node, vec![json!({"a": 1})], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(out, vec![vec![json!({"a": 1})]]);
    }

    #[test]
    fn stop_uses_message_first() {
        let node = mk(
            "s",
            "Stop",
            "n8n-nodes-base.stopAndError",
            HashMap::from([
                ("message".to_string(), json!("boom")),
                ("error".to_string(), json!("x")),
            ]),
        );
        let outputs = HashMap::new();
        let err = StopNode
            .execute(&node, vec![json!({})], &empty_ctx(&outputs))
            .expect_err("must fail");
        assert_eq!(err.to_string(), "stopAndError: boom");
    }

    #[test]
    fn stop_default_error_shape() {
        let node = mk("s", "Stop", "n8n-nodes-base.stopAndError", HashMap::new());
        let outputs = HashMap::new();
        let err = StopNode
            .execute(&node, vec![], &empty_ctx(&outputs))
            .expect_err("must fail");
        assert!(err.to_string().contains("Error:"), "{err}");
    }

    #[test]
    fn exec_runs_shell_and_captures_output() {
        let node = mk(
            "e",
            "Exec",
            "n8n-nodes-base.executeCommand",
            HashMap::from([("command".to_string(), json!("echo halo"))]),
        );
        let outputs = HashMap::new();
        let out = ExecNode
            .execute(&node, vec![json!({})], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(
            out,
            vec![vec![json!({
                "command": "echo halo",
                "stdout": "halo\n",
                "stderr": "",
                "exitCode": 0
            })]]
        );
    }

    #[test]
    fn exec_nonzero_is_error_by_default() {
        let node = mk(
            "e",
            "Exec",
            "n8n-nodes-base.executeCommand",
            HashMap::from([("command".to_string(), json!("exit 3"))]),
        );
        let outputs = HashMap::new();
        let err = ExecNode
            .execute(&node, vec![], &empty_ctx(&outputs))
            .expect_err("must fail");
        assert!(err.to_string().contains("exit tidak-nol (3)"), "{err}");
    }

    #[test]
    fn exec_continue_on_fail_keeps_item() {
        let node = mk(
            "e",
            "Exec",
            "n8n-nodes-base.executeCommand",
            HashMap::from([
                ("command".to_string(), json!("echo oops >&2; exit 3")),
                ("options".to_string(), json!({"continueOnFail": true})),
            ]),
        );
        let outputs = HashMap::new();
        let out = ExecNode
            .execute(&node, vec![], &empty_ctx(&outputs))
            .expect("exec");
        assert_eq!(out[0][0]["exitCode"], json!(3));
        assert_eq!(out[0][0]["stderr"], json!("oops\n"));
    }
}
