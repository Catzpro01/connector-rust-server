//! Merge (n8n Merge v3): `append`, `chooseBranch`, `combine`
//! (`combineByPosition`, `combineAll`, `combineByFields`).
//!
//! Implementasi orisinal meniru `Merge/v3` n8n asli. Input = tiap edge
//! masuk terpisah: input1 = pendahulu pertama di file, dst.
//! (`numberInputs` diabaikan — yang dipakai yang benar terhubung.)
//! `combineBySql` ditolak eksplisit (butuh SQL engine).
//!
//! Catatan default: `resolveClash` default = `addSuffix` khusus
//! combineByPosition (override n8n), `preferLast` untuk lainnya
//! (default helpers n8n) — bila opsi absen total, perilaku `{}` n8n
//! (last-wins, deep, tanpa sufiks) yang dipakai.

use n8n_engine::{BranchOutputs, EngineError, EngineResult};
use serde_json::{Map, Value};
use std::collections::HashMap;

pub fn run(inputs: &[Vec<Value>], params: &HashMap<String, Value>) -> EngineResult<BranchOutputs> {
    let mode = params
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("append");
    match mode {
        "append" => Ok(vec![inputs.iter().flatten().cloned().collect()]),
        "chooseBranch" => choose_branch(inputs, params),
        "combine" => combine(inputs, params),
        "combineBySql" => Err(EngineError::new(
            "merge: mode 'combineBySql' belum didukung (butuh SQL engine)",
        )),
        other => Err(EngineError::new(format!(
            "merge: mode tak dikenal '{other}'"
        ))),
    }
}

fn choose_branch(
    inputs: &[Vec<Value>],
    params: &HashMap<String, Value>,
) -> EngineResult<BranchOutputs> {
    let branch_mode = params
        .get("chooseBranchMode")
        .and_then(Value::as_str)
        .unwrap_or("waitForAll");
    if branch_mode != "waitForAll" {
        return Err(EngineError::new(format!(
            "merge: chooseBranchMode tak dikenal '{branch_mode}'"
        )));
    }
    let output = params
        .get("output")
        .and_then(Value::as_str)
        .unwrap_or("specifiedInput");
    match output {
        "specifiedInput" => {
            let n = params
                .get("useDataOfInput")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            if n < 1 || n > inputs.len() as i64 {
                return Err(EngineError::new(format!(
                    "merge: Input {n} doesn't exist (hanya {} input terhubung)",
                    inputs.len()
                )));
            }
            Ok(vec![inputs[(n - 1) as usize].clone()])
        }
        "empty" => Ok(vec![vec![Value::Object(Map::new())]]),
        other => Err(EngineError::new(format!(
            "merge: chooseBranch output tak dikenal '{other}'"
        ))),
    }
}

fn combine(inputs: &[Vec<Value>], params: &HashMap<String, Value>) -> EngineResult<BranchOutputs> {
    let by = params
        .get("combineBy")
        .and_then(Value::as_str)
        .unwrap_or("combineByFields");
    match by {
        "combineByPosition" => combine_position(inputs, params),
        "combineAll" => combine_all(inputs, params),
        "combineByFields" => combine_fields(inputs, params),
        other => Err(EngineError::new(format!(
            "merge: combineBy tak dikenal '{other}'"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Clash handling + deep/shallow merge ala lodash
// ---------------------------------------------------------------------------

struct ClashRaw {
    resolve: Option<String>,
    deep: bool,
    override_empty: bool,
}

fn read_clash(params: &HashMap<String, Value>, allow_prefer_n: bool) -> EngineResult<ClashRaw> {
    let values = params
        .get("options")
        .and_then(|o| o.get("clashHandling"))
        .and_then(|c| c.get("values"));
    let resolve = values
        .and_then(|v| v.get("resolveClash"))
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(r) = &resolve {
        let ok = matches!(r.as_str(), "addSuffix" | "preferInput1" | "preferLast")
            || (allow_prefer_n && r.starts_with("preferInput"));
        if !ok {
            return Err(EngineError::new(format!(
                "merge: resolveClash tak dikenal '{r}'"
            )));
        }
    }
    let deep = values
        .and_then(|v| v.get("mergeMode"))
        .and_then(Value::as_str)
        .map(|m| m != "shallowMerge")
        .unwrap_or(true);
    let override_empty = values
        .and_then(|v| v.get("overrideEmpty"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(ClashRaw {
        resolve,
        deep,
        override_empty,
    })
}

fn is_empty_scalar(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        _ => false,
    }
}

/// Gabung `src` ke `target` (rekursif bila deep; array digabung per
/// indeks ala lodash; override_empty: src null/string-kosong diabaikan).
fn merge_into(target: &mut Value, src: &Value, deep: bool, override_empty: bool) {
    if override_empty && is_empty_scalar(src) {
        return;
    }
    match (target, src) {
        (Value::Object(t), Value::Object(s)) => {
            for (k, v) in s {
                if deep {
                    merge_into(
                        t.entry(k.clone()).or_insert(Value::Null),
                        v,
                        deep,
                        override_empty,
                    );
                } else {
                    if override_empty && is_empty_scalar(v) {
                        continue;
                    }
                    t.insert(k.clone(), v.clone());
                }
            }
        }
        (Value::Array(t), Value::Array(s)) if deep => {
            for (i, v) in s.iter().enumerate() {
                if i < t.len() {
                    let mut cur = std::mem::replace(&mut t[i], Value::Null);
                    merge_into(&mut cur, v, deep, override_empty);
                    t[i] = cur;
                } else {
                    t.push(v.clone());
                }
            }
        }
        (t, s) => {
            *t = s.clone();
        }
    }
}

fn suffix_keys(v: &Value, suffix: &str) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (format!("{k}_{suffix}"), x.clone()))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn add_suffix(items: &[Value], suffix: &str) -> Vec<Value> {
    items.iter().map(|v| suffix_keys(v, suffix)).collect()
}

fn get_field<'a>(item: &'a Value, field: &str, dot: bool) -> Option<&'a Value> {
    if dot {
        crate::get_path(item, field)
    } else {
        item.get(field)
    }
}

// ---------------------------------------------------------------------------
// combineByPosition
// ---------------------------------------------------------------------------

fn combine_position(
    inputs: &[Vec<Value>],
    params: &HashMap<String, Value>,
) -> EngineResult<BranchOutputs> {
    if inputs.is_empty() {
        return Ok(vec![Vec::new()]);
    }
    let clash = read_clash(params, true)?;
    let include_unpaired = params
        .get("options")
        .and_then(|o| o.get("includeUnpaired"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let preferred_idx = match clash.resolve.as_deref() {
        Some(r) if r.contains("preferInput") => {
            let n: i64 = r
                .replace("preferInput", "")
                .parse()
                .map_err(|_| EngineError::new(format!("merge: resolveClash tak valid '{r}'")))?;
            if n < 1 || n > inputs.len() as i64 {
                return Err(EngineError::new(format!(
                    "merge: resolveClash '{r}' di luar jumlah input {}",
                    inputs.len()
                )));
            }
            (n - 1) as usize
        }
        _ => inputs.len() - 1,
    };
    let mut data: Vec<Vec<Value>> = inputs.to_vec();
    if clash.resolve.as_deref() == Some("addSuffix") {
        for (i, input) in data.iter_mut().enumerate() {
            *input = add_suffix(input, &(i + 1).to_string());
        }
    }
    let preferred = data[preferred_idx].clone();
    let num = if include_unpaired {
        data.iter().map(Vec::len).max().unwrap_or(0)
    } else {
        let m = data.iter().map(Vec::len).min().unwrap_or(0);
        if m == 0 {
            return Ok(vec![Vec::new()]);
        }
        m
    };
    let empty_obj = Value::Object(Map::new());
    let mut out = Vec::with_capacity(num);
    for i in 0..num {
        let mut acc = Value::Object(Map::new());
        for input in &data {
            let e = input.get(i).unwrap_or(&empty_obj);
            merge_into(&mut acc, e, clash.deep, clash.override_empty);
        }
        let pe = preferred.get(i).unwrap_or(&empty_obj);
        merge_into(&mut acc, pe, clash.deep, clash.override_empty);
        out.push(acc);
    }
    Ok(vec![out])
}

// ---------------------------------------------------------------------------
// combineAll (cross join 2 input pertama)
// ---------------------------------------------------------------------------

fn combine_all(
    inputs: &[Vec<Value>],
    params: &HashMap<String, Value>,
) -> EngineResult<BranchOutputs> {
    let (in1, in2) = match (inputs.first(), inputs.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Ok(vec![Vec::new()]),
    };
    let clash = read_clash(params, false)?;
    let (in1, in2) = if clash.resolve.as_deref() == Some("preferInput1") {
        (in2, in1)
    } else {
        (in1, in2)
    };
    let (in1, in2) = if clash.resolve.as_deref() == Some("addSuffix") {
        (add_suffix(&in1, "1"), add_suffix(&in2, "2"))
    } else {
        (in1, in2)
    };
    let mut out = Vec::with_capacity(in1.len() * in2.len());
    for e1 in &in1 {
        for e2 in &in2 {
            let mut acc = Value::Object(Map::new());
            merge_into(&mut acc, e1, clash.deep, clash.override_empty);
            merge_into(&mut acc, e2, clash.deep, clash.override_empty);
            out.push(acc);
        }
    }
    Ok(vec![out])
}

// ---------------------------------------------------------------------------
// combineByFields (join by key)
// ---------------------------------------------------------------------------

struct FoundMatches {
    matched: Vec<(Value, Vec<Value>)>,
    matched2: Vec<Value>,
    unmatched1: Vec<Value>,
    unmatched2: Vec<Value>,
}

fn check_match_fields(pairs: &[(String, String)]) -> EngineResult<()> {
    if pairs.len() == 1 && pairs[0].0.is_empty() && pairs[0].1.is_empty() {
        return Err(EngineError::new(
            "merge: You need to define at least one pair of fields in \"Fields to Match\" to match on",
        ));
    }
    for (i, (f1, f2)) in pairs.iter().enumerate() {
        if f1.is_empty() || f2.is_empty() {
            return Err(EngineError::new(format!(
                "merge: You need to define both fields in \"Fields to Match\" for pair {}, field 1 = '{f1}' field 2 = '{f2}'",
                i + 1
            )));
        }
    }
    Ok(())
}

fn combine_fields(
    inputs: &[Vec<Value>],
    params: &HashMap<String, Value>,
) -> EngineResult<BranchOutputs> {
    let advanced = params
        .get("advanced")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let pairs: Vec<(String, String)> = if advanced {
        params
            .get("mergeByFields")
            .and_then(|v| v.get("values"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|o| {
                        Some((
                            o.get("field1")?.as_str()?.to_string(),
                            o.get("field2")?.as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        params
            .get("fieldsToMatchString")
            .and_then(Value::as_str)
            .unwrap_or("")
            .split(',')
            .map(|f| {
                let t = f.trim().to_string();
                (t.clone(), t)
            })
            .collect()
    };
    check_match_fields(&pairs)?;
    let join = params
        .get("joinMode")
        .and_then(Value::as_str)
        .unwrap_or("keepMatches");
    if !matches!(
        join,
        "keepMatches" | "keepNonMatches" | "keepEverything" | "enrichInput1" | "enrichInput2"
    ) {
        return Err(EngineError::new(format!(
            "merge: joinMode tak dikenal '{join}'"
        )));
    }
    let output_from = params
        .get("outputDataFrom")
        .and_then(Value::as_str)
        .unwrap_or("both");
    if !matches!(output_from, "both" | "input1" | "input2") {
        return Err(EngineError::new(format!(
            "merge: outputDataFrom tak dikenal '{output_from}'"
        )));
    }
    let opts = params.get("options");
    let dot = !opts
        .and_then(|o| o.get("disableDotNotation"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let fuzzy = opts
        .and_then(|o| o.get("fuzzyCompare"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let multiple = opts
        .and_then(|o| o.get("multipleMatches"))
        .and_then(Value::as_str)
        .unwrap_or("all");
    if multiple != "all" && multiple != "first" {
        return Err(EngineError::new(format!(
            "merge: multipleMatches tak dikenal '{multiple}'"
        )));
    }
    let in1 = inputs.first().cloned().unwrap_or_default();
    let in2 = inputs.get(1).cloned().unwrap_or_default();
    if in1.is_empty() || in2.is_empty() {
        if join == "keepNonMatches" && output_from == "input1" && in1.is_empty() {
            return Ok(vec![Vec::new()]);
        }
        if join == "keepNonMatches" && output_from == "input2" && in2.is_empty() {
            return Ok(vec![Vec::new()]);
        }
        if join == "keepMatches" {
            return Ok(vec![]);
        }
        if join == "enrichInput1" && in1.is_empty() {
            return Ok(vec![]);
        }
        if join == "enrichInput2" && in2.is_empty() {
            return Ok(vec![]);
        }
        let mut cat = in1;
        cat.extend(in2);
        return Ok(vec![cat]);
    }
    if pairs.is_empty() {
        if matches!(join, "keepMatches" | "keepEverything" | "enrichInput2") {
            return Ok(vec![Vec::new()]);
        }
        return Ok(vec![in1]);
    }
    let m = find_matches(&in1, &in2, &pairs, dot, fuzzy, multiple);
    let clash = read_clash(params, false)?;
    match join {
        "keepMatches" | "keepEverything" => {
            let mut output = match output_from {
                "input1" => m.matched.iter().map(|(e, _)| e.clone()).collect(),
                "input2" => m.matched2.clone(),
                _ => {
                    let mut o = Vec::new();
                    for (e, ms) in &m.matched {
                        for m2 in ms {
                            o.push(merge_pair(e, std::slice::from_ref(m2), &clash, None));
                        }
                    }
                    o
                }
            };
            if join == "keepEverything" {
                let (mut u1, mut u2) = (m.unmatched1.clone(), m.unmatched2.clone());
                if clash.resolve.as_deref() == Some("addSuffix") {
                    u1 = add_suffix(&u1, "1");
                    u2 = add_suffix(&u2, "2");
                }
                output.extend(u1);
                output.extend(u2);
            }
            Ok(vec![output])
        }
        "keepNonMatches" => match output_from {
            "input1" => Ok(vec![m.unmatched1]),
            "input2" => Ok(vec![m.unmatched2]),
            _ => {
                let mut o = add_source(m.unmatched1, "input1");
                o.extend(add_source(m.unmatched2, "input2"));
                Ok(vec![o])
            }
        },
        "enrichInput1" | "enrichInput2" => {
            let mut output = Vec::new();
            for (e, ms) in &m.matched {
                for m2 in ms {
                    output.push(merge_pair(e, std::slice::from_ref(m2), &clash, Some(join)));
                }
            }
            let (un, suf) = if join == "enrichInput1" {
                (m.unmatched1, "1")
            } else {
                (m.unmatched2, "2")
            };
            if clash.resolve.as_deref() == Some("addSuffix") {
                output.extend(add_suffix(&un, suf));
            } else {
                output.extend(un);
            }
            Ok(vec![output])
        }
        _ => unreachable!(),
    }
}

fn merge_pair(entry: &Value, matches: &[Value], clash: &ClashRaw, join: Option<&str>) -> Value {
    let mut acc = Value::Object(Map::new());
    if clash.resolve.as_deref() == Some("addSuffix") {
        merge_into(
            &mut acc,
            &suffix_keys(entry, "1"),
            clash.deep,
            clash.override_empty,
        );
        for m in matches {
            merge_into(
                &mut acc,
                &suffix_keys(m, "2"),
                clash.deep,
                clash.override_empty,
            );
        }
        return acc;
    }
    let effective = clash.resolve.clone().unwrap_or_else(|| {
        if join == Some("enrichInput2") {
            "preferInput1".to_string()
        } else {
            "preferLast".to_string()
        }
    });
    if effective == "preferInput1" {
        let mut it = matches.iter();
        if let Some(first) = it.next() {
            merge_into(&mut acc, first, clash.deep, clash.override_empty);
            for m in it {
                merge_into(&mut acc, m, clash.deep, clash.override_empty);
            }
        }
        merge_into(&mut acc, entry, clash.deep, clash.override_empty);
    } else {
        merge_into(&mut acc, entry, clash.deep, clash.override_empty);
        for m in matches {
            merge_into(&mut acc, m, clash.deep, clash.override_empty);
        }
    }
    acc
}

fn add_source(items: Vec<Value>, source: &str) -> Vec<Value> {
    items
        .into_iter()
        .map(|v| match v {
            Value::Object(mut o) => {
                o.insert("_source".to_string(), Value::String(source.to_string()));
                Value::Object(o)
            }
            other => serde_json::json!({"value": other, "_source": source}),
        })
        .collect()
}

fn find_matches(
    in1: &[Value],
    in2: &[Value],
    pairs: &[(String, String)],
    dot: bool,
    fuzzy: bool,
    multiple: &str,
) -> FoundMatches {
    let mut matched = Vec::new();
    let mut unmatched1 = Vec::new();
    let mut matched_idx: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for e1 in in1 {
        let mut lookup = Vec::with_capacity(pairs.len());
        let mut missing = false;
        for (f1, f2) in pairs {
            match get_field(e1, f1, dot) {
                Some(v) => lookup.push((f2.clone(), v.clone())),
                None => {
                    missing = true;
                    break;
                }
            }
        }
        if missing {
            unmatched1.push(e1.clone());
            continue;
        }
        let mut hits = Vec::new();
        for (j, e2) in in2.iter().enumerate() {
            let ok = lookup
                .iter()
                .all(|(f2, want)| match get_field(e2, f2, dot) {
                    Some(got) => entries_equal(want, got, fuzzy),
                    None => fuzzy && is_falsy(want),
                });
            if ok {
                hits.push(j);
                if multiple == "first" {
                    break;
                }
            }
        }
        if hits.is_empty() {
            unmatched1.push(e1.clone());
        } else {
            let ms: Vec<Value> = hits
                .iter()
                .map(|&j| {
                    matched_idx.insert(j);
                    in2[j].clone()
                })
                .collect();
            matched.push((e1.clone(), ms));
        }
    }
    let mut matched2 = Vec::new();
    let mut unmatched2 = Vec::new();
    for (j, e2) in in2.iter().enumerate() {
        if matched_idx.contains(&j) {
            matched2.push(e2.clone());
        } else {
            unmatched2.push(e2.clone());
        }
    }
    FoundMatches {
        matched,
        matched2,
        unmatched1,
        unmatched2,
    }
}

fn json_kind(v: &Value) -> u8 {
    match v {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(_) => 2,
        Value::String(_) => 3,
        Value::Array(_) => 4,
        Value::Object(_) => 5,
    }
}

fn entries_equal(a: &Value, b: &Value, fuzzy: bool) -> bool {
    if !fuzzy {
        return a == b;
    }
    if json_kind(a) == json_kind(b) {
        return a == b;
    }
    match (a, b) {
        (Value::Number(n), Value::String(s)) => n.to_string() == *s,
        (Value::String(s), Value::Number(n)) => *s == n.to_string(),
        _ => is_falsy(a) && is_falsy(b),
    }
}

/// Aproksimasi `isFalsy` n8n: null, false, 0, string/array/object kosong.
fn is_falsy(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64().map(|f| f == 0.0).unwrap_or(false),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn p(pairs: Vec<(&str, Value)>) -> HashMap<String, Value> {
        pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    #[test]
    fn append_concatenates_in_order() {
        let out = run(
            &[
                vec![json!({"a": 1})],
                vec![json!({"b": 2}), json!({"b": 3})],
            ],
            &p(vec![]),
        )
        .expect("run");
        assert_eq!(
            out,
            vec![vec![json!({"a": 1}), json!({"b": 2}), json!({"b": 3})]]
        );
    }

    #[test]
    fn choose_branch_selects_input() {
        let inputs = vec![vec![json!({"a": 1})], vec![json!({"b": 2})]];
        let out = run(
            &inputs,
            &p(vec![
                ("mode", json!("chooseBranch")),
                ("useDataOfInput", json!(2)),
            ]),
        )
        .expect("run");
        assert_eq!(out, vec![vec![json!({"b": 2})]]);
        let empty = run(
            &inputs,
            &p(vec![
                ("mode", json!("chooseBranch")),
                ("output", json!("empty")),
            ]),
        )
        .expect("run");
        assert_eq!(empty, vec![vec![json!({})]]);
        let err = run(
            &inputs,
            &p(vec![
                ("mode", json!("chooseBranch")),
                ("useDataOfInput", json!(5)),
            ]),
        )
        .expect_err("must fail");
        assert!(err.to_string().contains("doesn't exist"), "{err}");
    }

    #[test]
    fn position_pairs_and_suffixes() {
        let out = run(
            &[vec![json!({"x": 1})], vec![json!({"x": 9})]],
            &p(vec![
                ("mode", json!("combine")),
                ("combineBy", json!("combineByPosition")),
                (
                    "options",
                    json!({"clashHandling": {"values": {"resolveClash": "addSuffix"}}}),
                ),
            ]),
        )
        .expect("run");
        assert_eq!(out, vec![vec![json!({"x_1": 1, "x_2": 9})]]);
    }

    #[test]
    fn position_include_unpaired() {
        let out = run(
            &[
                vec![json!({"a": 1}), json!({"a": 2})],
                vec![json!({"b": 9})],
            ],
            &p(vec![
                ("mode", json!("combine")),
                ("combineBy", json!("combineByPosition")),
                ("options", json!({"includeUnpaired": true})),
            ]),
        )
        .expect("run");
        assert_eq!(out, vec![vec![json!({"a": 1, "b": 9}), json!({"a": 2})]]);
    }

    #[test]
    fn all_cross_joins_first_two() {
        let out = run(
            &[
                vec![json!({"a": 1}), json!({"a": 2})],
                vec![json!({"b": 9})],
            ],
            &p(vec![
                ("mode", json!("combine")),
                ("combineBy", json!("combineAll")),
            ]),
        )
        .expect("run");
        assert_eq!(
            out,
            vec![vec![json!({"a": 1, "b": 9}), json!({"a": 2, "b": 9})]]
        );
    }

    #[test]
    fn fields_keep_matches_and_enrich() {
        let inputs = vec![
            vec![json!({"id": 1, "x": "a"}), json!({"id": 2, "x": "b"})],
            vec![json!({"id": 1, "z": "Z"})],
        ];
        let keep = run(
            &inputs,
            &p(vec![
                ("mode", json!("combine")),
                ("combineBy", json!("combineByFields")),
                ("fieldsToMatchString", json!("id")),
            ]),
        )
        .expect("run");
        assert_eq!(keep, vec![vec![json!({"id": 1, "x": "a", "z": "Z"})]]);
        let enrich = run(
            &inputs,
            &p(vec![
                ("mode", json!("combine")),
                ("combineBy", json!("combineByFields")),
                ("fieldsToMatchString", json!("id")),
                ("joinMode", json!("enrichInput1")),
            ]),
        )
        .expect("run");
        assert_eq!(
            enrich,
            vec![vec![
                json!({"id": 1, "x": "a", "z": "Z"}),
                json!({"id": 2, "x": "b"})
            ]]
        );
    }

    #[test]
    fn fields_keep_non_matches_both_marks_source() {
        let inputs = vec![
            vec![json!({"id": 1}), json!({"id": 2})],
            vec![json!({"id": 1}), json!({"id": 3})],
        ];
        let out = run(
            &inputs,
            &p(vec![
                ("mode", json!("combine")),
                ("combineBy", json!("combineByFields")),
                ("fieldsToMatchString", json!("id")),
                ("joinMode", json!("keepNonMatches")),
            ]),
        )
        .expect("run");
        assert_eq!(
            out,
            vec![vec![
                json!({"id": 2, "_source": "input1"}),
                json!({"id": 3, "_source": "input2"})
            ]]
        );
    }

    #[test]
    fn sql_is_rejected_and_match_fields_validated() {
        let err = run(&[], &p(vec![("mode", json!("combineBySql"))])).expect_err("sql");
        assert!(err.to_string().contains("combineBySql"), "{err}");
        let err = run(
            &[vec![json!({"a": 1})], vec![json!({"a": 1})]],
            &p(vec![
                ("mode", json!("combine")),
                ("combineBy", json!("combineByFields")),
                ("fieldsToMatchString", json!("")),
            ]),
        )
        .expect_err("empty pair");
        assert!(err.to_string().contains("at least one pair"), "{err}");
    }
}
