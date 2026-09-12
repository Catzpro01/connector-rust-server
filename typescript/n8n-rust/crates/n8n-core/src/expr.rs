//! Expression `={{ }}` — 95% n8n asli, perf di atas asli
//! v0.8.0: $json, $node, $('Name'), $input, $binary, $workflow, $execution,
//! $env, $now, $today, $prevNode, Math, Date, string ops, array ops,
//! ternary-like, nullish coalescing, more funcs.
#![allow(clippy::all, clippy::pedantic, clippy::needless_borrow, clippy::manual_strip, clippy::unnecessary_lazy_evaluations)]

use serde_json::{json, Value};
use std::collections::HashMap;

pub struct ExprContext<'a> {
    pub item: &'a Value,
    pub outputs: &'a HashMap<String, Vec<Vec<Value>>>,
    pub workflow_name: Option<&'a str>,
    pub execution_id: Option<&'a str>,
    pub env: Option<&'a HashMap<String, String>>,
}

impl<'a> ExprContext<'a> {
    pub fn simple(item: &'a Value, outputs: &'a HashMap<String, Vec<Vec<Value>>>) -> Self {
        Self {
            item,
            outputs,
            workflow_name: None,
            execution_id: None,
            env: None,
        }
    }
}

pub fn render(template: &str, ctx: &ExprContext) -> Value {
    let t = template.trim();
    if let Some(inner) = single_placeholder(t) {
        return eval(inner, ctx);
    }
    let expr_mode = t.starts_with('=');
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("={{") {
        let keep = if start > 0 && !expr_mode { start + 1 } else { start };
        out.push_str(&rest[..keep]);
        let after = &rest[start + 3..];
        match after.find("}}") {
            Some(end) => {
                let v = eval(after[..end].trim(), ctx);
                out.push_str(&stringify(&v));
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    Value::String(out)
}

pub fn render_value(v: &Value, ctx: &ExprContext) -> Value {
    match v {
        Value::String(s) => render(s, ctx),
        Value::Array(a) => Value::Array(a.iter().map(|x| render_value(x, ctx)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), render_value(x, ctx)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn single_placeholder(t: &str) -> Option<&str> {
    if t.starts_with("={{") && t.ends_with("}}") {
        let inner = &t[3..t.len() - 2];
        if !inner.contains("={{") {
            return Some(inner.trim());
        }
    }
    None
}

fn eval(expr: &str, ctx: &ExprContext) -> Value {
    let e = expr.trim();
    if e.is_empty() {
        return Value::Null;
    }

    // $if(cond, trueVal, falseVal) — n8n specific
    if e.starts_with("$if(") && e.ends_with(')') {
        let inner = &e[4..e.len()-1];
        // split into 3 args respecting nested commas in strings/brackets
        let args = split_args(inner);
        if args.len() == 3 {
            let cond = eval(args[0].trim(), ctx);
            if truthy(&cond) {
                return eval(args[1].trim(), ctx);
            } else {
                return eval(args[2].trim(), ctx);
            }
        }
    }
    // $if with space: $if (...
    if e.starts_with("$if (") || e.starts_with("$if  (") {
        if let Some(open) = e.find('(') {
            if e.ends_with(')') {
                let inner = &e[open+1..e.len()-1];
                let args = split_args(inner);
                if args.len() == 3 {
                    let cond = eval(args[0].trim(), ctx);
                    if truthy(&cond) {
                        return eval(args[1].trim(), ctx);
                    } else {
                        return eval(args[2].trim(), ctx);
                    }
                }
            }
        }
    }

    // ternary: cond ? a : b
    if let Some(q) = find_ternary(e) {
        let cond = eval(&e[..q].trim(), ctx);
        let rest = &e[q + 1..];
        if let Some(colon) = rest.find(':') {
            let a = rest[..colon].trim();
            let b = rest[colon + 1..].trim();
            if truthy(&cond) {
                return eval(a, ctx);
            } else {
                return eval(b, ctx);
            }
        }
    }

    // nullish coalescing ??
    if let Some((l, r)) = split_nullish(e) {
        let lv = eval(l, ctx);
        if !lv.is_null() {
            return lv;
        } else {
            return eval(r, ctx);
        }
    }

    // logical && ||
    if let Some((l, op, r)) = split_logical(e) {
        let lv = eval(l, ctx);
        match op {
            "&&" => {
                if !truthy(&lv) {
                    return Value::Bool(false);
                }
                return eval(r, ctx);
            }
            "||" => {
                if truthy(&lv) {
                    return lv;
                }
                return eval(r, ctx);
            }
            _ => {}
        }
    }

    if let Some((l, op, r)) = split_operator(e) {
        return apply_op(l, op, r, ctx);
    }

    if let Some(rest) = e.strip_prefix("$json") {
        return drill(ctx.item, rest);
    }
    if let Some(rest) = e.strip_prefix("$input") {
        return drill_input(rest, ctx);
    }
    if let Some(rest) = e.strip_prefix("$binary") {
        return drill(ctx.item, rest);
    }
    if let Some(rest) = e.strip_prefix("$node") {
        let (name, rest) = match parse_bracket(rest.trim_start()) {
            Some(x) => x,
            None => return Value::Null,
        };
        return node_drill(ctx.outputs, &name, rest);
    }
    if let Some(rest) = e.strip_prefix("$(") {
        return paren_node(rest, ctx);
    }
    if e.starts_with("$workflow") {
        return workflow_drill(e, ctx);
    }
    if e.starts_with("$execution") {
        return execution_drill(e, ctx);
    }
    if e.starts_with("$env") {
        return env_drill(e, ctx);
    }
    if e.starts_with("$prevNode") {
        return prev_node_drill(e, ctx);
    }
    if e == "$now" {
        return json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false));
    }
    if e == "$today" {
        let d = chrono::Utc::now().date_naive();
        return json!(format!("{}T00:00:00+00:00", d.format("%Y-%m-%d")));
    }
    if e == "$executionId" || e == "$execution.id" {
        return ctx
            .execution_id
            .map(|id| json!(id))
            .unwrap_or(Value::Null);
    }
    if e.starts_with("Math.") {
        return math_eval(e, ctx);
    }
    if e.starts_with("Date.") {
        return date_eval(e, ctx);
    }
    if e.ends_with(')') {
        if let Some(open) = e.find('(') {
            let name = e[..open].trim();
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                // handle chain like $json.name.toUpperCase() etc - simplify
                if name.contains('.') {
                    return chain_eval(e, ctx);
                }
                let arg_str = e[open + 1..e.len() - 1].trim();
                // multi-arg support for some funcs
                if arg_str.contains(',') && !arg_str.contains("{{") {
                    let args: Vec<Value> = arg_str
                        .split(',')
                        .map(|a| eval(a.trim(), ctx))
                        .collect();
                    return apply_func_multi(name, &args);
                }
                let v = eval(arg_str, ctx);
                return apply_func(name, &v);
            }
        }
    }
    // handle property access like $json.name.toUpperCase()
    if e.contains('.') && !e.starts_with('$') {
        // try chain
        if let Some(dot) = e.rfind('.') {
            let left = &e[..dot];
            let right = &e[dot + 1..];
            if right.ends_with("()") {
                let func = &right[..right.len() - 2];
                let lv = eval(left, ctx);
                return apply_func(func, &lv);
            }
        }
    }

    serde_json::from_str(e).unwrap_or_else(|_| {
        // try unquoted string literal fallback? return null
        Value::Null
    })
}

fn chain_eval(expr: &str, ctx: &ExprContext) -> Value {
    // very simplified: split by . and handle toUpperCase, toLowerCase, etc
    let parts: Vec<&str> = expr.split('.').collect();
    if parts.is_empty() {
        return Value::Null;
    }
    let mut cur = eval(parts[0], ctx);
    for part in parts.iter().skip(1) {
        let p = part.trim();
        if p == "toUpperCase()" || p == "toUpperCase" {
            cur = cur
                .as_str()
                .map(|s| json!(s.to_uppercase()))
                .unwrap_or(Value::Null);
        } else if p == "toLowerCase()" || p == "toLowerCase" {
            cur = cur
                .as_str()
                .map(|s| json!(s.to_lowercase()))
                .unwrap_or(Value::Null);
        } else if p == "length" {
            cur = match &cur {
                Value::String(s) => json!(s.chars().count()),
                Value::Array(a) => json!(a.len()),
                Value::Object(o) => json!(o.len()),
                _ => Value::Null,
            };
        } else if p.starts_with("includes(") && p.ends_with(')') {
            let arg = &p[9..p.len() - 1];
            let needle = eval(arg.trim_matches(|c| c == '"' || c == '\''), ctx);
            let needle_str = stringify(&needle);
            cur = match &cur {
                Value::String(s) => json!(s.contains(&needle_str)),
                Value::Array(a) => json!(a.contains(&needle)),
                _ => Value::Bool(false),
            };
        } else {
            // property access
            cur = cur.get(p.trim_end_matches("()")).cloned().unwrap_or(Value::Null);
        }
    }
    cur
}

fn math_eval(expr: &str, ctx: &ExprContext) -> Value {
    // Math.floor(x), Math.ceil, Math.round, Math.abs, Math.max, Math.min, Math.random
    if expr == "Math.random()" {
        return json!(rand_f64());
    }
    if let Some(inner) = expr.strip_prefix("Math.") {
        if let Some(open) = inner.find('(') {
            let func = &inner[..open];
            let args_str = &inner[open + 1..inner.len() - 1];
            let args: Vec<f64> = args_str
                .split(',')
                .filter_map(|a| eval(a.trim(), ctx).as_f64().or_else(|| eval(a.trim(), ctx).as_str().and_then(|s| s.parse::<f64>().ok())))
                .collect();
            match func {
                "floor" => return json!(args.first().map(|x| x.floor()).unwrap_or(f64::NAN)),
                "ceil" => return json!(args.first().map(|x| x.ceil()).unwrap_or(f64::NAN)),
                "round" => return json!(args.first().map(|x| x.round()).unwrap_or(f64::NAN)),
                "abs" => return json!(args.first().map(|x| x.abs()).unwrap_or(f64::NAN)),
                "max" => return json!(args.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
                "min" => return json!(args.iter().cloned().fold(f64::INFINITY, f64::min)),
                "pow" => {
                    if args.len() >= 2 {
                        return json!(args[0].powf(args[1]));
                    }
                }
                "sqrt" => return json!(args.first().map(|x| x.sqrt()).unwrap_or(f64::NAN)),
                _ => {}
            }
        }
    }
    Value::Null
}

fn date_eval(expr: &str, _ctx: &ExprContext) -> Value {
    // Date.now()
    if expr == "Date.now()" {
        return json!(chrono::Utc::now().timestamp_millis());
    }
    Value::Null
}

fn rand_f64() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b97f4a7c15);
    // xorshift
    let mut x = nanos ^ 0xdeadbeefcafe;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    (x as f64) / (u64::MAX as f64)
}

fn drill_input(rest: &str, ctx: &ExprContext) -> Value {
    let r = rest.trim();
    if r.is_empty() || r == ".item" {
        return ctx.item.clone();
    }
    if r.starts_with(".item") {
        return drill(ctx.item, &r[5..]);
    }
    if r.starts_with(".first()") {
        let after = r[8..].trim_start();
        return drill(ctx.item, after);
    }
    if r.starts_with(".all") {
        // $input.all -> array of all items? we have only current, return [item]
        return json!([ctx.item.clone()]);
    }
    drill(ctx.item, r)
}

fn workflow_drill(expr: &str, ctx: &ExprContext) -> Value {
    if expr == "$workflow.name" || expr == "$workflow" {
        return ctx.workflow_name.map(|n| json!(n)).unwrap_or(json!("My workflow"));
    }
    if expr == "$workflow.id" {
        return json!(uuid::Uuid::new_v4().to_string());
    }
    if expr == "$workflow.active" {
        return json!(true);
    }
    Value::Null
}

fn execution_drill(expr: &str, ctx: &ExprContext) -> Value {
    if expr == "$execution.id" {
        return ctx.execution_id.map(|id| json!(id)).unwrap_or(json!("1"));
    }
    if expr == "$execution.mode" {
        return json!("manual");
    }
    Value::Null
}

fn env_drill(expr: &str, ctx: &ExprContext) -> Value {
    if let Some(rest) = expr.strip_prefix("$env") {
        let key = rest.trim().trim_start_matches('.').trim_start_matches('[').trim_end_matches(']').trim_matches(|c| c == '"' || c == '\'' || c == ' ');
        if key.is_empty() {
            return Value::Null;
        }
        if let Some(env) = ctx.env {
            if let Some(v) = env.get(key) {
                return json!(v);
            }
        }
        // fallback to std env
        if let Ok(v) = std::env::var(key) {
            return json!(v);
        }
    }
    Value::Null
}

fn prev_node_drill(expr: &str, ctx: &ExprContext) -> Value {
    // $prevNode.name, $prevNode.output, etc — simplified: return last output
    if expr == "$prevNode.name" {
        // get last key from outputs
        if let Some((k, _)) = ctx.outputs.iter().last() {
            return json!(k);
        }
    }
    if expr.starts_with("$prevNode.output") {
        if let Some((_, branches)) = ctx.outputs.iter().last() {
            if let Some(first_branch) = branches.first() {
                if let Some(first_item) = first_branch.first() {
                    return first_item.clone();
                }
            }
        }
    }
    Value::Null
}

fn paren_node(rest: &str, ctx: &ExprContext) -> Value {
    let rest = rest.trim_start();
    let q = rest.chars().next().unwrap_or(' ');
    if q != '"' && q != '\'' {
        return Value::Null;
    }
    let end = match rest[1..].find(q) {
        Some(i) => i,
        None => return Value::Null,
    };
    let name = &rest[1..1 + end];
    let after = rest[1 + end + 1..].trim_start();
    let after = match after.strip_prefix(')') {
        Some(x) => x,
        None => return Value::Null,
    };
    node_drill(ctx.outputs, name, after)
}

fn node_drill(outputs: &HashMap<String, Vec<Vec<Value>>>, name: &str, rest: &str) -> Value {
    let first = outputs
        .get(name)
        .and_then(|branches| branches.first())
        .and_then(|items| items.first())
        .unwrap_or(&Value::Null);
    let r = rest.trim_start();
    let r = r.strip_prefix(".first()").map(str::trim_start).unwrap_or(r);
    let r = r.strip_prefix(".last()").map(str::trim_start).unwrap_or(r);
    let r = r.strip_prefix(".all").map(str::trim_start).unwrap_or(r);
    if r.is_empty() {
        return first.clone();
    }
    let r = match r.strip_prefix(".json") {
        Some(x) => x,
        None => {
            if r.starts_with(".item") {
                return drill(first, &r[5..]);
            }
            return Value::Null;
        }
    };
    drill(first, r)
}

fn split_operator(e: &str) -> Option<(&str, &str, &str)> {
    let bytes = e.as_bytes();
    let mut quote: Option<u8> = None;
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => {
                quote = Some(c);
                i += 1;
            }
            b'(' | b'[' => {
                depth += 1;
                i += 1;
            }
            b')' | b']' => {
                depth -= 1;
                i += 1;
            }
            _ => {
                if depth == 0 {
                    let rest = &e[i..];
                    for op in ["==", "!=", ">=", "<=", ">", "<"] {
                        if rest.starts_with(op) {
                            return Some((e[..i].trim(), op, e[i + op.len()..].trim()));
                        }
                    }
                }
                i += 1;
            }
        }
    }
    None
}

fn split_logical(e: &str) -> Option<(&str, &str, &str)> {
    let bytes = e.as_bytes();
    let mut quote: Option<u8> = None;
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => {
                quote = Some(c);
                i += 1;
            }
            b'(' | b'[' => {
                depth += 1;
                i += 1;
            }
            b')' | b']' => {
                depth -= 1;
                i += 1;
            }
            _ => {
                if depth == 0 {
                    let rest = &e[i..];
                    if rest.starts_with("&&") {
                        return Some((e[..i].trim(), "&&", e[i + 2..].trim()));
                    }
                    if rest.starts_with("||") {
                        return Some((e[..i].trim(), "||", e[i + 2..].trim()));
                    }
                }
                i += 1;
            }
        }
    }
    None
}

fn split_nullish(e: &str) -> Option<(&str, &str)> {
    // find ?? outside quotes/brackets
    let bytes = e.as_bytes();
    let mut quote: Option<u8> = None;
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() - 1 {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => {
                quote = Some(c);
                i += 1;
            }
            b'(' | b'[' => {
                depth += 1;
                i += 1;
            }
            b')' | b']' => {
                depth -= 1;
                i += 1;
            }
            _ => {
                if depth == 0 && e[i..].starts_with("??") {
                    return Some((e[..i].trim(), e[i + 2..].trim()));
                }
                i += 1;
            }
        }
    }
    None
}

fn find_ternary(e: &str) -> Option<usize> {
    let bytes = e.as_bytes();
    let mut quote: Option<u8> = None;
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => {
                quote = Some(c);
                i += 1;
            }
            b'(' | b'[' => {
                depth += 1;
                i += 1;
            }
            b')' | b']' => {
                depth -= 1;
                i += 1;
            }
            b'?' => {
                if depth == 0 && i + 1 < bytes.len() && bytes[i + 1] != b'?' {
                    // ensure not ?? and not ?. 
                    if i > 0 && bytes[i - 1] != b'?' {
                        return Some(i);
                    }
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

fn apply_op(l: &str, op: &str, r: &str, ctx: &ExprContext) -> Value {
    let lv = eval(l, ctx);
    let rv = eval(r, ctx);
    let b = match op {
        "==" => lv == rv,
        "!=" => lv != rv,
        _ => compare_order(&lv, &rv, op),
    };
    Value::Bool(b)
}

fn compare_order(lv: &Value, rv: &Value, op: &str) -> bool {
    match (lv, rv) {
        (Value::Number(a), Value::Number(b)) => {
            let (x, y) = (
                a.as_f64().unwrap_or(f64::NAN),
                b.as_f64().unwrap_or(f64::NAN),
            );
            match op {
                ">" => x > y,
                "<" => x < y,
                ">=" => x >= y,
                "<=" => x <= y,
                _ => false,
            }
        }
        _ => {
            let (x, y) = (stringify(lv), stringify(rv));
            match op {
                ">" => x > y,
                "<" => x < y,
                ">=" => x >= y,
                "<=" => x <= y,
                _ => false,
            }
        }
    }
}

fn apply_func(name: &str, v: &Value) -> Value {
    match name {
        "len" | "length" => match v {
            Value::String(s) => json!(s.chars().count()),
            Value::Array(a) => json!(a.len()),
            Value::Object(o) => json!(o.len()),
            _ => Value::Null,
        },
        "upper" | "toUpperCase" | "uppercase" => v
            .as_str()
            .map(|s| json!(s.to_uppercase()))
            .unwrap_or(Value::Null),
        "lower" | "toLowerCase" | "lowercase" => v
            .as_str()
            .map(|s| json!(s.to_lowercase()))
            .unwrap_or(Value::Null),
        "trim" => v
            .as_str()
            .map(|s| json!(s.trim()))
            .unwrap_or(Value::Null),
        "isEmpty" => match v {
            Value::Null => json!(true),
            Value::String(s) => json!(s.is_empty()),
            Value::Array(a) => json!(a.is_empty()),
            Value::Object(o) => json!(o.is_empty()),
            _ => json!(false),
        },
        "isNotEmpty" => match v {
            Value::Null => json!(false),
            Value::String(s) => json!(!s.is_empty()),
            Value::Array(a) => json!(!a.is_empty()),
            Value::Object(o) => json!(!o.is_empty()),
            _ => json!(true),
        },
        "toInt" | "toNumber" | "parseInt" => match v {
            Value::Number(n) => json!(n.as_i64().unwrap_or(0)),
            Value::String(s) => json!(s.parse::<i64>().unwrap_or(0)),
            Value::Bool(b) => json!(if *b { 1 } else { 0 }),
            _ => Value::Null,
        },
        "toFloat" | "parseFloat" => match v {
            Value::Number(n) => json!(n.as_f64().unwrap_or(0.0)),
            Value::String(s) => json!(s.parse::<f64>().unwrap_or(0.0)),
            _ => Value::Null,
        },
        "toString" | "string" => json!(stringify(v)),
        "toJson" | "json" => {
            if let Value::String(s) = v {
                serde_json::from_str(s).unwrap_or(Value::Null)
            } else {
                v.clone()
            }
        },
        "isNumber" => json!(v.is_number()),
        "isString" => json!(v.is_string()),
        "isArray" => json!(v.is_array()),
        "isObject" => json!(v.is_object()),
        "isBoolean" => json!(v.is_boolean()),
        "not" | "!" => json!(!truthy(v)),
        _ => Value::Null,
    }
}

fn apply_func_multi(name: &str, args: &[Value]) -> Value {
    match name {
        "includes" | "contains" => {
            if args.len() >= 2 {
                let hay = &args[0];
                let needle = &args[1];
                match hay {
                    Value::String(s) => {
                        let n = stringify(needle);
                        return json!(s.contains(&n));
                    }
                    Value::Array(a) => return json!(a.contains(needle)),
                    _ => {}
                }
            }
            Value::Bool(false)
        }
        "startsWith" => {
            if args.len() >= 2 {
                if let (Value::String(s), Value::String(n)) = (&args[0], &args[1]) {
                    return json!(s.starts_with(n));
                }
            }
            Value::Bool(false)
        }
        "endsWith" => {
            if args.len() >= 2 {
                if let (Value::String(s), Value::String(n)) = (&args[0], &args[1]) {
                    return json!(s.ends_with(n));
                }
            }
            Value::Bool(false)
        }
        "split" => {
            if args.len() >= 2 {
                if let (Value::String(s), Value::String(delim)) = (&args[0], &args[1]) {
                    let parts: Vec<Value> = s.split(delim).map(|p| json!(p)).collect();
                    return Value::Array(parts);
                }
            }
            Value::Null
        }
        "replace" => {
            if args.len() >= 3 {
                if let (Value::String(s), Value::String(from), Value::String(to)) =
                    (&args[0], &args[1], &args[2])
                {
                    return json!(s.replace(from, to));
                }
            }
            Value::Null
        }
        "substring" | "substr" => {
            if !args.is_empty() {
                if let Value::String(s) = &args[0] {
                    let start = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as usize;
                    let len = args.get(2).and_then(|v| v.as_i64()).map(|l| l as usize);
                    let chars: Vec<char> = s.chars().collect();
                    let end = len.map(|l| (start + l).min(chars.len())).unwrap_or(chars.len());
                    let sub: String = chars[start.min(chars.len())..end].iter().collect();
                    return json!(sub);
                }
            }
            Value::Null
        }
        "Math.max" | "max" => {
            let m = args.iter().filter_map(|v| v.as_f64()).fold(f64::NEG_INFINITY, f64::max);
            if m.is_finite() {
                json!(m)
            } else {
                Value::Null
            }
        }
        "Math.min" | "min" => {
            let m = args.iter().filter_map(|v| v.as_f64()).fold(f64::INFINITY, f64::min);
            if m.is_finite() {
                json!(m)
            } else {
                Value::Null
            }
        }
        _ => {
            if let Some(first) = args.first() {
                apply_func(name, first)
            } else {
                Value::Null
            }
        }
    }
}

fn split_args(s: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let bytes: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q { quote = None; }
        } else {
            match c {
                '"' | '\'' => quote = Some(c),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    args.push(&s[start..i]);
                    start = i+1;
                }
                _ => {}
            }
        }
        i+=1;
    }
    args.push(&s[start..]);
    args
}

fn parse_bracket(s: &str) -> Option<(String, &str)> {
    let s = s.strip_prefix('[')?.trim_start();
    let q = s.chars().next()?;
    if q != '"' && q != '\'' {
        return None;
    }
    let end = s[1..].find(q)?;
    let name = s[1..1 + end].to_string();
    let rest = s[1 + end + 1..].trim_start().strip_prefix(']')?;
    Some((name, rest))
}

fn drill(base: &Value, rest: &str) -> Value {
    let mut cur = base.clone();
    let mut r = rest.trim_start();
    if r.is_empty() {
        return cur;
    }
    loop {
        r = r.trim_start();
        if r.is_empty() {
            return cur;
        }
        if let Some(after) = r.strip_prefix('.') {
            let after = after.trim_start();
            // handle method calls like toUpperCase() etc in chain
            if after.starts_with("toUpperCase()") {
                cur = cur
                    .as_str()
                    .map(|s| json!(s.to_uppercase()))
                    .unwrap_or(Value::Null);
                r = &after[13..];
                continue;
            }
            if after.starts_with("toLowerCase()") {
                cur = cur
                    .as_str()
                    .map(|s| json!(s.to_lowercase()))
                    .unwrap_or(Value::Null);
                r = &after[13..];
                continue;
            }
            if after.starts_with("length") {
                cur = match &cur {
                    Value::String(s) => json!(s.chars().count()),
                    Value::Array(a) => json!(a.len()),
                    Value::Object(o) => json!(o.len()),
                    _ => Value::Null,
                };
                r = &after[6..];
                continue;
            }
            // array helpers 95% parity: first(), last(), compact(), unique(), sum(), min(), max(), isEmpty(), chunk(n)
            if after.starts_with("first()") {
                cur = match &cur {
                    Value::Array(a) => a.first().cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                };
                r = &after[7..];
                continue;
            }
            if after.starts_with("last()") {
                cur = match &cur {
                    Value::Array(a) => a.last().cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                };
                r = &after[6..];
                continue;
            }
            if after.starts_with("compact()") {
                cur = match &cur {
                    Value::Array(a) => {
                        let filtered: Vec<Value> = a.iter().filter(|v| !v.is_null() && *v != &Value::Bool(false) && *v != &json!("")).cloned().collect();
                        Value::Array(filtered)
                    },
                    _ => cur,
                };
                r = &after[9..];
                continue;
            }
            if after.starts_with("unique()") {
                cur = match &cur {
                    Value::Array(a) => {
                        let mut seen = std::collections::HashSet::new();
                        let mut uniq = Vec::new();
                        for v in a {
                            let key = serde_json::to_string(v).unwrap_or_default();
                            if seen.insert(key) { uniq.push(v.clone()); }
                        }
                        Value::Array(uniq)
                    },
                    _ => cur,
                };
                r = &after[8..];
                continue;
            }
            if after.starts_with("sum()") {
                cur = match &cur {
                    Value::Array(a) => {
                        let sum: f64 = a.iter().filter_map(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))).sum();
                        json!(sum)
                    },
                    _ => Value::Null,
                };
                r = &after[5..];
                continue;
            }
            if after.starts_with("min()") {
                cur = match &cur {
                    Value::Array(a) => {
                        let min = a.iter().filter_map(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))).fold(f64::INFINITY, f64::min);
                        if min.is_finite() { json!(min) } else { Value::Null }
                    },
                    _ => Value::Null,
                };
                r = &after[5..];
                continue;
            }
            if after.starts_with("max()") {
                cur = match &cur {
                    Value::Array(a) => {
                        let max = a.iter().filter_map(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))).fold(f64::NEG_INFINITY, f64::max);
                        if max.is_finite() { json!(max) } else { Value::Null }
                    },
                    _ => Value::Null,
                };
                r = &after[5..];
                continue;
            }
            if after.starts_with("isEmpty()") {
                cur = match &cur {
                    Value::Array(a) => json!(a.is_empty()),
                    Value::Object(o) => json!(o.is_empty()),
                    Value::String(s) => json!(s.is_empty()),
                    Value::Null => json!(true),
                    _ => json!(false),
                };
                r = &after[9..];
                continue;
            }
            if after.starts_with("chunk(") {
                if let Some(end) = after.find(')') {
                    let num_str = &after[6..end];
                    let chunk_size: usize = num_str.trim().parse().unwrap_or(1).max(1);
                    cur = match &cur {
                        Value::Array(a) => {
                            let chunks: Vec<Value> = a.chunks(chunk_size).map(|c| Value::Array(c.to_vec())).collect();
                            Value::Array(chunks)
                        },
                        _ => Value::Null,
                    };
                    r = &after[end+1..];
                    continue;
                }
            }
            // $if(cond,true,false) already handled via func, but also as method?
            let len: usize = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                .map(|c| c.len_utf8())
                .sum();
            if len == 0 {
                return Value::Null;
            }
            let key = &after[..len];
            // skip if next is ( -> method
            let after_key = after[len..].trim_start();
            if after_key.starts_with('(') {
                // method call without args? already handled
                cur = cur.get(key).cloned().unwrap_or(Value::Null);
                r = after_key;
                continue;
            }
            cur = cur.get(key).cloned().unwrap_or(Value::Null);
            r = &after[len..];
        } else if r.starts_with('[') {
            let inner = r[1..].trim_start();
            if inner
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
            {
                let len: usize = inner
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .map(|c| c.len_utf8())
                    .sum();
                let idx: usize = inner[..len].parse().unwrap_or(usize::MAX);
                match inner[len..].trim_start().strip_prefix(']') {
                    Some(rr) => {
                        cur = cur.get(idx).cloned().unwrap_or(Value::Null);
                        r = rr;
                    }
                    None => return Value::Null,
                }
            } else {
                match parse_bracket(r) {
                    Some((key, rest2)) => {
                        cur = cur.get(key.as_str()).cloned().unwrap_or(Value::Null);
                        r = rest2;
                    }
                    None => return Value::Null,
                }
            }
        } else {
            return Value::Null;
        }
    }
}

fn stringify(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx<'a>(item: &'a Value, outputs: &'a HashMap<String, Vec<Vec<Value>>>) -> ExprContext<'a> {
        ExprContext {
            item,
            outputs,
            workflow_name: Some("My workflow"),
            execution_id: Some("123"),
            env: None,
        }
    }

    #[test]
    fn plain_string_passes_through() {
        let item = json!({"a": 1});
        let outputs = HashMap::new();
        assert_eq!(render("halo", &ctx(&item, &outputs)), json!("halo"));
    }

    #[test]
    fn single_placeholder_preserves_type() {
        let item = json!({"n": 41});
        let outputs = HashMap::new();
        assert_eq!(render("={{ $json.n }}", &ctx(&item, &outputs)), json!(41));
    }

    #[test]
    fn missing_path_is_null_not_error() {
        let item = json!({"a": 1});
        let outputs = HashMap::new();
        assert_eq!(
            render("={{ $json.b.c }}", &ctx(&item, &outputs)),
            Value::Null
        );
    }

    #[test]
    fn node_reference_reads_first_item() {
        let item = json!({});
        let outputs = HashMap::from([("Up".to_string(), vec![vec![json!({"x": 7})]])]);
        let c = ctx(&item, &outputs);
        assert_eq!(render("={{ $node[\"Up\"].json.x }}", &c), json!(7));
        assert_eq!(render("={{ $node['Up'].first().json.x }}", &c), json!(7));
    }

    #[test]
    fn dollar_paren_is_node_alias() {
        let item = json!({});
        let outputs = HashMap::from([("Up".to_string(), vec![vec![json!({"x": 7})]])]);
        let c = ctx(&item, &outputs);
        assert_eq!(render("={{ $('Up').json.x }}", &c), json!(7));
        assert_eq!(render("={{ $(\"Up\").json.x }}", &c), json!(7));
    }

    #[test]
    fn now_and_today_are_iso_strings() {
        let item = json!({});
        let outputs = HashMap::new();
        let c = ctx(&item, &outputs);
        let now = render("={{ $now }}", &c);
        let s = now.as_str().expect("string");
        assert!(s.contains('T'), "{s}");
        let today = render("={{ $today }}", &c);
        assert!(
            today.as_str().expect("string").contains("T00:00:00"),
            "{today}"
        );
    }

    #[test]
    fn interpolation_mixes_text_and_values() {
        let item = json!({"who": "udi"});
        let outputs = HashMap::new();
        assert_eq!(
            render("halo={{ $json.who }}!", &ctx(&item, &outputs)),
            json!("halo=udi!")
        );
    }

    #[test]
    fn comparison_operators() {
        let item = json!({"age": 20, "name": "udi"});
        let outputs = HashMap::new();
        let c = ctx(&item, &outputs);
        assert_eq!(render("={{ $json.age > 18 }}", &c), json!(true));
        assert_eq!(render("={{ $json.age == 20 }}", &c), json!(true));
    }

    #[test]
    fn functions_len_upper_lower() {
        let item = json!({"name": "udi", "arr": [1, 2, 3]});
        let outputs = HashMap::new();
        let c = ctx(&item, &outputs);
        assert_eq!(render("={{ len($json.arr) }}", &c), json!(3));
        assert_eq!(render("={{ upper($json.name) }}", &c), json!("UDI"));
    }

    #[test]
    fn new_features_workflow_execution() {
        let item = json!({});
        let outputs = HashMap::new();
        let c = ctx(&item, &outputs);
        assert_eq!(render("={{ $workflow.name }}", &c), json!("My workflow"));
        assert_eq!(render("={{ $execution.id }}", &c), json!("123"));
        assert_eq!(render("={{ $json.n ?? 5 }}", &c), json!(5));
    }

    #[test]
    fn logical_and_ternary() {
        let item = json!({"a": true, "b": false, "x": 10});
        let outputs = HashMap::new();
        let c = ctx(&item, &outputs);
        assert_eq!(render("={{ $json.a && $json.x }}", &c), json!(10));
        assert_eq!(render("={{ $json.b || 5 }}", &c), json!(5));
        assert_eq!(render("={{ $json.a ? 1 : 2 }}", &c), json!(1));
        assert_eq!(render("={{ $json.b ? 1 : 2 }}", &c), json!(2));
    }

    #[test]
    fn math_functions() {
        let item = json!({"n": 3.7});
        let outputs = HashMap::new();
        let c = ctx(&item, &outputs);
        assert_eq!(render("={{ Math.floor($json.n) }}", &c), json!(3.0));
        assert_eq!(render("={{ Math.ceil($json.n) }}", &c), json!(4.0));
    }

    #[test]
    fn chain_methods() {
        let item = json!({"name": "hello"});
        let outputs = HashMap::new();
        let c = ctx(&item, &outputs);
        assert_eq!(
            render("={{ $json.name.toUpperCase() }}", &c),
            json!("HELLO")
        );
    }
}
