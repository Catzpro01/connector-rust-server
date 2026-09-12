//! Evaluator `conditions` ala n8n (dipakai node If + Filter).
//!
//! Implementasi orisinal yang meniru semantik
//! `n8n-workflow/src/node-parameters/filter-parameter.ts` milik n8n asli:
//! - bentuk: `{combinator: 'and'|'or', conditions: [...], options: {...}}`
//! - tiap rule: `{leftValue, rightValue, operator: {type, operation}}`
//! - tipe: string number dateTime boolean array object (+ any)
//! - `caseSensitive` default false (n8n: `ignoreCase` default true)
//! - validasi tipe default STRICT (n8n If/Filter v2: looseTypeValidation
//!   default false); longgar via `conditions.options.typeValidation='loose'`
//!   atau `options.looseTypeValidation=true`
//! - string kosong → number = null (perilaku n8n version ≥ 3, terbaru)
//!
//! Perbedaan disengaja vs n8n (terdokumentasi, bukan bug):
//! - operator/combinator tak dikenal → ERROR eksplisit (n8n: warn + false)
//! - perbandingan angka dengan null → false (n8n: koersi JS, mis. null<1)
//! - `array contains` memakai kesetaraan mendalam untuk object (n8n:
//!   SameValueZero JS = referensi untuk object)
//! - regex tanpa pelindung timeout (n8n: safe-regex); pola tepercaya saja
//! - dateTime: subset ISO-8601/RFC3339 + milidetik epoch (n8n: luxon penuh)

use n8n_engine::{EngineError, EngineResult};
use serde_json::Value;

pub struct FilterOpts {
    pub case_sensitive: bool,
    pub strict: bool,
}

/// Opsi efektif: `conditions.options.*` menang bila ada, lalu node
/// `options` (`ignoreCase` default true, `looseTypeValidation` default
/// false — sama seperti n8n), lalu default n8n.
pub fn resolve_opts(conditions: &Value, node_options: Option<&Value>) -> FilterOpts {
    let c_opts = conditions.get("options");
    let n_opts = node_options.and_then(Value::as_object);
    let case_sensitive = c_opts
        .and_then(|o| o.get("caseSensitive"))
        .and_then(Value::as_bool)
        .or_else(|| {
            n_opts
                .and_then(|o| o.get("ignoreCase"))
                .and_then(Value::as_bool)
                .map(|ic| !ic)
        })
        .unwrap_or(false);
    let strict = c_opts
        .and_then(|o| o.get("typeValidation"))
        .and_then(Value::as_str)
        .map(|s| s != "loose")
        .or_else(|| {
            n_opts
                .and_then(|o| o.get("looseTypeValidation"))
                .and_then(Value::as_bool)
                .map(|loose| !loose)
        })
        .unwrap_or(true);
    FilterOpts {
        case_sensitive,
        strict,
    }
}

/// Nilai `conditions` + renderer ekspresi per item (dipanggil untuk tiap
/// leftValue/rightValue — n8n me-resolve `={{ }}` sebelum evaluasi).
pub fn evaluate(
    conditions: &Value,
    item_index: usize,
    render: &dyn Fn(&Value) -> Value,
    opts: &FilterOpts,
) -> EngineResult<bool> {
    let combinator = conditions
        .get("combinator")
        .and_then(Value::as_str)
        .unwrap_or("and");
    if combinator != "and" && combinator != "or" {
        return Err(EngineError::new(format!(
            "conditions: combinator tak dikenal '{combinator}'"
        )));
    }
    let list = conditions
        .get("conditions")
        .and_then(Value::as_array)
        .ok_or_else(|| EngineError::new("conditions: 'conditions[]' hilang"))?;
    let mut results = Vec::with_capacity(list.len());
    for (i, cond) in list.iter().enumerate() {
        results.push(eval_condition(cond, i, item_index, render, opts)?);
    }
    if combinator == "and" {
        Ok(results.into_iter().all(|b| b))
    } else {
        Ok(results.into_iter().any(|b| b))
    }
}

fn single_value_op(op: &str, type_: &str) -> bool {
    matches!(op, "empty" | "notEmpty" | "exists" | "notExists")
        || (type_ == "boolean" && matches!(op, "true" | "false"))
}

fn eval_condition(
    cond: &Value,
    index: usize,
    item_index: usize,
    render: &dyn Fn(&Value) -> Value,
    opts: &FilterOpts,
) -> EngineResult<bool> {
    let tag = format!("[condition {index}, item {item_index}]");
    let operator = cond
        .get("operator")
        .and_then(Value::as_object)
        .ok_or_else(|| EngineError::new(format!("conditions: operator hilang {tag}")))?;
    let type_ = operator
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| EngineError::new(format!("conditions: operator.type hilang {tag}")))?;
    let operation = operator
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(|| EngineError::new(format!("conditions: operator.operation hilang {tag}")))?;
    // Operasi panjang membandingkan angka (len vs rightValue) — kanan
    // harus number kecuali rightType eksplisit (n8n: input angka).
    let right_default = if type_ == "array" && operation.starts_with("length") {
        "number"
    } else {
        type_
    };
    let right_type = operator
        .get("rightType")
        .and_then(Value::as_str)
        .unwrap_or(right_default);

    let left_raw = render(cond.get("leftValue").unwrap_or(&Value::Null));
    let left = coerce(&left_raw, type_, opts.strict, &tag)?;
    let right = if single_value_op(operation, type_) {
        Value::Null
    } else {
        let right_raw = render(cond.get("rightValue").unwrap_or(&Value::Null));
        coerce(&right_raw, right_type, opts.strict, &tag)?
    };

    let exists = !left.is_null();
    if operation == "exists" {
        return Ok(exists);
    }
    if operation == "notExists" {
        return Ok(!exists);
    }

    let ignore_case = !opts.case_sensitive;
    match type_ {
        "string" => eval_string(&left, &right, operation, ignore_case, &tag),
        "number" => eval_number(&left, &right, operation, exists, &tag),
        "dateTime" => eval_datetime(&left, &right, operation, exists, &tag),
        "boolean" => eval_boolean(&left, &right, operation, exists, &tag),
        "array" => eval_array(&left, &right, operation, ignore_case, &tag),
        "object" => eval_object(&left, operation, &tag),
        "any" => Err(EngineError::new(format!(
            "conditions: tipe 'any' tanpa implementasi operasi {tag}"
        ))),
        other => Err(EngineError::new(format!(
            "conditions: tipe tak dikenal '{other}' {tag}"
        ))),
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn with_article(noun: &str) -> String {
    let art = if noun.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("{art} {noun}")
}

/// Koersi longgar/ketat ala `parseSingleFilterValue` + `validateFieldType`.
fn coerce(v: &Value, type_: &str, strict: bool, tag: &str) -> EngineResult<Value> {
    if type_ == "any" || v.is_null() {
        return Ok(v.clone());
    }
    let fail = |from: &str| {
        EngineError::new(if strict {
            format!(
                "Wrong type: '{}' is {} but was expecting {} {tag}",
                display(v),
                with_article(from),
                with_article(type_)
            )
        } else {
            format!(
                "Conversion error: the {from} '{}' can't be converted to {} {tag} (aktifkan 'Convert types where required' / periksa tipe)",
                display(v),
                with_article(type_)
            )
        })
    };
    match type_ {
        "string" => match v {
            Value::String(_) => Ok(v.clone()),
            Value::Number(n) if !strict => Ok(Value::String(n.to_string())),
            Value::Bool(b) if !strict => Ok(Value::String(b.to_string())),
            _ => Err(fail(type_name(v))),
        },
        "number" => match v {
            Value::Number(_) => Ok(v.clone()),
            Value::String(s) if !strict => {
                let t = s.trim();
                if t.is_empty() {
                    return Ok(Value::Null);
                }
                t.parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .ok_or_else(|| fail("string"))
            }
            Value::Bool(b) if !strict => Ok(serde_json::json!(if *b { 1 } else { 0 })),
            _ => Err(fail(type_name(v))),
        },
        "boolean" => match v {
            Value::Bool(_) => Ok(v.clone()),
            _ if !strict => Ok(Value::Bool(js_truthy(v))),
            _ => Err(fail(type_name(v))),
        },
        "dateTime" => parse_datetime(v).ok_or_else(|| fail(type_name(v))),
        "array" => match v {
            Value::Array(_) => Ok(v.clone()),
            _ => Err(fail(type_name(v))),
        },
        "object" => match v {
            Value::Object(_) => Ok(v.clone()),
            _ => Err(fail(type_name(v))),
        },
        "any" => Ok(v.clone()),
        _ => Err(EngineError::new(format!(
            "conditions: tipe tak dikenal '{type_}' {tag}"
        ))),
    }
}

/// `Boolean(x)` ala JavaScript (dipakai n8n untuk boolean longgar).
fn js_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0 && !f.is_nan()).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Parse tanggal → `Number` milidetik epoch. Terima RFC3339/ISO-8601,
/// `YYYY-MM-DDTHH:MM:SS[.f]` (UTC), `YYYY-MM-DD` (tengah malam UTC),
/// atau angka milidetik.
fn parse_datetime(v: &Value) -> Option<Value> {
    match v {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .map(|ms| serde_json::json!(ms)),
        Value::String(s) => {
            let t = s.trim();
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(t) {
                return Some(serde_json::json!(dt.timestamp_millis()));
            }
            if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S%.f") {
                return Some(serde_json::json!(naive.and_utc().timestamp_millis()));
            }
            if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M:%S") {
                return Some(serde_json::json!(naive.and_utc().timestamp_millis()));
            }
            if let Ok(date) = chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d") {
                return Some(serde_json::json!(date
                    .and_hms_opt(0, 0, 0)
                    .map(|d| d.and_utc().timestamp_millis())
                    .unwrap_or(0)));
            }
            None
        }
        _ => None,
    }
}

fn lower_opt(v: &Value, ignore_case: bool) -> Value {
    if ignore_case {
        if let Value::String(s) = v {
            return Value::String(s.to_lowercase());
        }
    }
    v.clone()
}

fn eval_string(
    left: &Value,
    right: &Value,
    operation: &str,
    ignore_case: bool,
    tag: &str,
) -> EngineResult<bool> {
    let l = lower_opt(left, ignore_case);
    let l = l.as_str().unwrap_or("");
    let regex_op = matches!(operation, "regex" | "notRegex");
    let r = lower_opt(right, ignore_case && !regex_op);
    let r = r.as_str().unwrap_or("");
    match operation {
        "empty" => Ok(l.is_empty()),
        "notEmpty" => Ok(!l.is_empty()),
        "equals" => Ok(l == r),
        "notEquals" => Ok(l != r),
        "contains" => Ok(l.contains(r)),
        "notContains" => Ok(!l.contains(r)),
        "startsWith" => Ok(l.starts_with(r)),
        "notStartsWith" => Ok(!l.starts_with(r)),
        "endsWith" => Ok(l.ends_with(r)),
        "notEndsWith" => Ok(!l.ends_with(r)),
        "regex" => Ok(regex_test(r, l, tag)?),
        "notRegex" => Ok(!regex_test(r, l, tag)?),
        other => Err(EngineError::new(format!(
            "conditions: operasi string tak dikenal '{other}' {tag}"
        ))),
    }
}

/// Pola `/pola/flags` (flags i/m/s; g/u/y diabaikan) atau pola mentah.
fn regex_test(pattern: &str, input: &str, tag: &str) -> EngineResult<bool> {
    let (source, flags) = match parse_regex_literal(pattern) {
        Some((s, f)) => (s, f),
        None => (pattern, ""),
    };
    let mut built = String::new();
    if flags.contains('i') {
        built.push_str("(?i)");
    }
    if flags.contains('m') {
        built.push_str("(?m)");
    }
    if flags.contains('s') {
        built.push_str("(?s)");
    }
    built.push_str(source);
    let re = regex::Regex::new(&built)
        .map_err(|e| EngineError::new(format!("conditions: regex tak valid: {e} {tag}")))?;
    Ok(re.is_match(input))
}

fn parse_regex_literal(literal: &str) -> Option<(&str, &str)> {
    if !literal.starts_with('/') {
        return None;
    }
    let end = literal[1..].rfind('/')?;
    let flags = &literal[1 + end + 1..];
    if !flags.chars().all(|c| "gimusy".contains(c)) {
        return None;
    }
    Some((&literal[1..1 + end], flags))
}

fn eval_number(
    left: &Value,
    right: &Value,
    operation: &str,
    exists: bool,
    tag: &str,
) -> EngineResult<bool> {
    match operation {
        "empty" => return Ok(!exists),
        "notEmpty" => return Ok(exists),
        _ => {}
    }
    let (l, r) = match (left.as_f64(), right.as_f64()) {
        (Some(a), Some(b)) => (a, b),
        _ => {
            return match operation {
                "equals" => Ok(left.is_null() && right.is_null()),
                "notEquals" => Ok(!(left.is_null() && right.is_null())),
                "gt" | "lt" | "gte" | "lte" => Ok(false),
                other => Err(EngineError::new(format!(
                    "conditions: operasi number tak dikenal '{other}' {tag}"
                ))),
            }
        }
    };
    match operation {
        "equals" => Ok(l == r),
        "notEquals" => Ok(l != r),
        "gt" => Ok(l > r),
        "lt" => Ok(l < r),
        "gte" => Ok(l >= r),
        "lte" => Ok(l <= r),
        other => Err(EngineError::new(format!(
            "conditions: operasi number tak dikenal '{other}' {tag}"
        ))),
    }
}

fn eval_datetime(
    left: &Value,
    right: &Value,
    operation: &str,
    exists: bool,
    tag: &str,
) -> EngineResult<bool> {
    match operation {
        "empty" => return Ok(!exists),
        "notEmpty" => return Ok(exists),
        _ => {}
    }
    let (l, r) = match (left.as_i64(), right.as_i64()) {
        (Some(a), Some(b)) => (a, b),
        _ => return Ok(false),
    };
    match operation {
        "equals" => Ok(l == r),
        "notEquals" => Ok(l != r),
        "after" => Ok(l > r),
        "before" => Ok(l < r),
        "afterOrEquals" => Ok(l >= r),
        "beforeOrEquals" => Ok(l <= r),
        other => Err(EngineError::new(format!(
            "conditions: operasi dateTime tak dikenal '{other}' {tag}"
        ))),
    }
}

fn eval_boolean(
    left: &Value,
    right: &Value,
    operation: &str,
    exists: bool,
    tag: &str,
) -> EngineResult<bool> {
    match operation {
        "empty" => return Ok(!exists),
        "notEmpty" => return Ok(exists),
        "true" => return Ok(left.as_bool().unwrap_or(false)),
        "false" => return Ok(!left.as_bool().unwrap_or(false)),
        _ => {}
    }
    match (left.as_bool(), right.as_bool()) {
        (Some(l), Some(r)) => match operation {
            "equals" => Ok(l == r),
            "notEquals" => Ok(l != r),
            other => Err(EngineError::new(format!(
                "conditions: operasi boolean tak dikenal '{other}' {tag}"
            ))),
        },
        _ => match operation {
            "equals" => Ok(left == right),
            "notEquals" => Ok(left != right),
            other => Err(EngineError::new(format!(
                "conditions: operasi boolean tak dikenal '{other}' {tag}"
            ))),
        },
    }
}

fn eval_array(
    left: &Value,
    right: &Value,
    operation: &str,
    ignore_case: bool,
    tag: &str,
) -> EngineResult<bool> {
    let arr: &[Value] = left.as_array().map(Vec::as_slice).unwrap_or(&[]);
    match operation {
        "contains" => Ok(array_contains(arr, right, ignore_case)),
        "notContains" => Ok(!array_contains(arr, right, ignore_case)),
        "empty" => Ok(arr.is_empty()),
        "notEmpty" => Ok(!arr.is_empty()),
        "lengthEquals" | "lengthNotEquals" | "lengthGt" | "lengthLt" | "lengthGte"
        | "lengthLte" => {
            let n = match right.as_f64() {
                Some(f) => f,
                None => return Ok(false),
            };
            let len = arr.len() as f64;
            match operation {
                "lengthEquals" => Ok(len == n),
                "lengthNotEquals" => Ok(len != n),
                "lengthGt" => Ok(len > n),
                "lengthLt" => Ok(len < n),
                "lengthGte" => Ok(len >= n),
                "lengthLte" => Ok(len <= n),
                _ => unreachable!(),
            }
        }
        other => Err(EngineError::new(format!(
            "conditions: operasi array tak dikenal '{other}' {tag}"
        ))),
    }
}

fn array_contains(arr: &[Value], value: &Value, ignore_case: bool) -> bool {
    if ignore_case {
        if let Value::String(want) = value {
            let want = want.to_lowercase();
            return arr.iter().any(|item| match item {
                Value::String(s) => s.to_lowercase() == want,
                _ => false,
            });
        }
    }
    arr.iter().any(|item| item == value)
}

fn eval_object(left: &Value, operation: &str, tag: &str) -> EngineResult<bool> {
    let empty = match left {
        Value::Object(o) => o.is_empty(),
        _ => true,
    };
    match operation {
        "empty" => Ok(empty),
        "notEmpty" => Ok(!empty),
        other => Err(EngineError::new(format!(
            "conditions: operasi object tak dikenal '{other}' {tag}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ident(v: &Value) -> Value {
        v.clone()
    }

    fn run(conds: &Value, node_options: Option<&Value>) -> EngineResult<bool> {
        let opts = resolve_opts(conds, node_options);
        evaluate(conds, 0, &ident, &opts)
    }

    #[test]
    fn string_ops_case_insensitive_by_default() {
        let conds = json!({
            "combinator": "and",
            "conditions": [
                {"leftValue": "Halo Dunia", "rightValue": "halo",
                 "operator": {"type": "string", "operation": "startsWith"}},
                {"leftValue": "Halo Dunia", "rightValue": "DUNIA",
                 "operator": {"type": "string", "operation": "contains"}}
            ],
            "options": {}
        });
        assert!(run(&conds, None).expect("eval"));
    }

    #[test]
    fn combinator_or_and_not_variants() {
        let conds = json!({
            "combinator": "or",
            "conditions": [
                {"leftValue": "a", "rightValue": "b",
                 "operator": {"type": "string", "operation": "equals"}},
                {"leftValue": "a", "rightValue": "b",
                 "operator": {"type": "string", "operation": "notEquals"}}
            ],
            "options": {"caseSensitive": true}
        });
        assert!(run(&conds, None).expect("eval"));
    }

    #[test]
    fn number_comparison_and_strict_default() {
        let conds = json!({
            "combinator": "and",
            "conditions": [
                {"leftValue": 20, "rightValue": 18,
                 "operator": {"type": "number", "operation": "gt"}}
            ],
            "options": {}
        });
        assert!(run(&conds, None).expect("eval"));
        let bad = json!({
            "combinator": "and",
            "conditions": [
                {"leftValue": "20", "rightValue": 18,
                 "operator": {"type": "number", "operation": "gt"}}
            ],
            "options": {}
        });
        let err = run(&bad, None).expect_err("strict harus gagal");
        assert!(err.to_string().contains("Wrong type"), "{err}");
    }

    #[test]
    fn loose_mode_converts_types() {
        let conds = json!({
            "combinator": "and",
            "conditions": [
                {"leftValue": "20", "rightValue": 18,
                 "operator": {"type": "number", "operation": "gt"}},
                {"leftValue": "", "rightValue": 1,
                 "operator": {"type": "number", "operation": "empty"}}
            ],
            "options": {"typeValidation": "loose"}
        });
        assert!(run(&conds, None).expect("eval"));
    }

    #[test]
    fn exists_and_boolean_single_value() {
        let conds = json!({
            "combinator": "and",
            "conditions": [
                {"leftValue": null, "operator": {"type": "string", "operation": "notExists"}},
                {"leftValue": true, "operator": {"type": "boolean", "operation": "true"}}
            ],
            "options": {}
        });
        assert!(run(&conds, None).expect("eval"));
    }

    #[test]
    fn datetime_after_and_array_length() {
        let conds = json!({
            "combinator": "and",
            "conditions": [
                {"leftValue": "2026-09-11T00:00:00+00:00", "rightValue": "2026-01-01",
                 "operator": {"type": "dateTime", "operation": "after"}},
                {"leftValue": [1, 2, 3], "rightValue": 2,
                 "operator": {"type": "array", "operation": "lengthGt"}}
            ],
            "options": {}
        });
        assert!(run(&conds, None).expect("eval"));
    }

    #[test]
    fn regex_with_flags() {
        let conds = json!({
            "combinator": "and",
            "conditions": [
                {"leftValue": "abc123", "rightValue": "/^a.*\\d+$/",
                 "operator": {"type": "string", "operation": "regex"}}
            ],
            "options": {"caseSensitive": true}
        });
        assert!(run(&conds, None).expect("eval"));
    }

    #[test]
    fn unknown_operator_is_explicit_error() {
        let conds = json!({
            "combinator": "and",
            "conditions": [
                {"leftValue": "a", "rightValue": "a",
                 "operator": {"type": "string", "operation": "fuzzy"}}
            ],
            "options": {}
        });
        assert!(run(&conds, None).is_err());
    }
}
