//! DateTime (n8n DateTime V2): 7 operasi tanggal/waktu.
//!
//! Implementasi orisinal meniru `DateTime/V2` n8n asli. Zona kerja =
//! UTC; zona IANA lain (mis. "Asia/Jakarta") didukung via chrono-tz untuk
//! `getCurrentDate.options.timezone`. Output string = RFC3339 milidetik
//! (setara `toString()` luxon n8n).
//!
//! Subset vs n8n (luxon+moment): parse TANPA fuzzy ("tomorrow" ditolak);
//! `getTimeBetweenDates` menolak month/year (butuh kalender); durasi
//! pecahan unit kalender (years/quarters/months) dipotong ke integer;
//! `fromFormat` placeholder default n8n ('e.g yyyyMMdd') diabaikan.

use chrono::{Datelike, TimeZone, Timelike};
use chrono_tz::Tz;
use n8n_engine::{EngineError, EngineResult};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Hitung SATU operasi → (namaFieldOutput, nilai).
pub fn compute(
    params: &HashMap<String, Value>,
    render: &dyn Fn(&Value) -> Value,
) -> EngineResult<(String, Value)> {
    let op = params
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or("getCurrentDate");
    match op {
        "getCurrentDate" => op_current(params),
        "addToDate" => op_add(params, render, 1.0),
        "subtractFromDate" => op_add(params, render, -1.0),
        "formatDate" => op_format(params, render),
        "roundDate" => op_round(params, render),
        "getTimeBetweenDates" => op_between(params, render),
        "extractDate" => op_extract(params, render),
        other => Err(EngineError::new(format!(
            "dateTime: operation tak dikenal '{other}'"
        ))),
    }
}

fn field(params: &HashMap<String, Value>, def: &str) -> String {
    params
        .get("outputFieldName")
        .and_then(Value::as_str)
        .unwrap_or(def)
        .to_string()
}

fn resolve_tz(name: &str) -> EngineResult<Tz> {
    if name.is_empty() || name == "UTC" || name == "Etc/UTC" {
        return Ok(chrono_tz::UTC);
    }
    name.parse::<Tz>().map_err(|_| {
        EngineError::new(format!(
            "dateTime: The timezone {name} is not valid. Please check the timezone."
        ))
    })
}

fn out(dt: &chrono::DateTime<Tz>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, false)
}

fn as_number_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                return None;
            }
            t.parse::<f64>().ok()
        }
        _ => None,
    }
}

fn from_local(naive: chrono::NaiveDateTime, tz: Tz) -> EngineResult<chrono::DateTime<Tz>> {
    tz.from_local_datetime(&naive)
        .single()
        .ok_or_else(|| EngineError::new("dateTime: waktu ambigu/tak ada di zona ini"))
}

/// Parse ala n8n `parseDate`: angka (pecahan→×1000 ms; integer <12
/// digit→detik, else ms), ISO/RFC3339, `fromFormat` token luxon.
fn parse_dt(v: &Value, from_format: Option<&str>, tz: Tz) -> EngineResult<chrono::DateTime<Tz>> {
    if from_format.is_none() {
        if let Some(f) = as_number_f64(v) {
            let ms: i64 = if f.fract() != 0.0 {
                (f * 1000.0) as i64
            } else {
                let i = f as i64;
                if i.to_string().len() < 12 {
                    i.saturating_mul(1000)
                } else {
                    i
                }
            };
            let dt = chrono::DateTime::from_timestamp_millis(ms)
                .ok_or_else(|| EngineError::new("dateTime: Invalid date format"))?;
            return Ok(dt.with_timezone(&tz));
        }
    }
    let s = v
        .as_str()
        .ok_or_else(|| EngineError::new("dateTime: Invalid date format"))?;
    let t = s.trim();
    if let Some(fmt) = from_format {
        let chrono_fmt = luxon_to_chrono(fmt)?;
        let naive = chrono::NaiveDateTime::parse_from_str(t, &chrono_fmt)
            .map_err(|_| EngineError::new("dateTime: Invalid date format"))?;
        return from_local(naive, tz);
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(t) {
        return Ok(dt.with_timezone(&tz));
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S%.f") {
        return from_local(naive, tz);
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M:%S") {
        return from_local(naive, tz);
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        let naive = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| EngineError::new("dateTime: Invalid date format"))?;
        return from_local(naive, tz);
    }
    Err(EngineError::new("dateTime: Invalid date format"))
}

/// Token luxon → chrono (subset; terpanjang dulu; `'...'` = literal).
fn luxon_to_chrono(fmt: &str) -> EngineResult<String> {
    let mut out = String::new();
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\'' {
            if chars.get(i + 1) == Some(&'\'') {
                out.push('\'');
                i += 2;
                continue;
            }
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '\'' {
                j += 1;
            }
            if j >= chars.len() {
                return Err(EngineError::new(
                    "dateTime: kutip tak berpasangan di format",
                ));
            }
            out.push_str(&chars[i + 1..j].iter().collect::<String>());
            i = j + 1;
            continue;
        }
        let rest: String = chars[i..].iter().collect();
        let mut hit: Option<(&str, &str)> = None;
        for (tok, chrono) in [
            ("yyyy", "%Y"),
            ("yy", "%y"),
            ("MMMM", "%B"),
            ("MMM", "%b"),
            ("MM", "%m"),
            ("M", "%-m"),
            ("dd", "%d"),
            ("d", "%-d"),
            ("HH", "%H"),
            ("H", "%-H"),
            ("hh", "%I"),
            ("h", "%-I"),
            ("mm", "%M"),
            ("ss", "%S"),
            ("SSS", "%3f"),
            ("a", "%P"),
            ("X", "%s"),
            ("ZZZZ", "%Z"),
            ("ZZZ", "%:z"),
            ("ZZ", "%:z"),
            ("Z", "%:z"),
        ] {
            if rest.starts_with(tok) {
                hit = Some((tok, chrono));
                break;
            }
        }
        match hit {
            Some((tok, chrono)) => {
                out.push_str(chrono);
                i += tok.len();
            }
            None => {
                let c = chars[i];
                if c.is_ascii_alphabetic() {
                    return Err(EngineError::new(format!(
                        "dateTime: token format tak didukung '{c}'"
                    )));
                }
                if c == '%' {
                    out.push_str("%%");
                } else {
                    out.push(c);
                }
                i += 1;
            }
        }
    }
    Ok(out)
}

fn op_current(params: &HashMap<String, Value>) -> EngineResult<(String, Value)> {
    let include_time = params
        .get("includeTime")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let tz_name = params
        .get("options")
        .and_then(|o| o.get("timezone"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let tz = resolve_tz(tz_name)?;
    let now = chrono::Utc::now().with_timezone(&tz);
    let s = if include_time {
        out(&now)
    } else {
        let naive = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| EngineError::new("dateTime: gagal tengah malam"))?;
        out(&from_local(naive, tz)?)
    };
    Ok((field(params, "currentDate"), Value::String(s)))
}

fn op_add(
    params: &HashMap<String, Value>,
    render: &dyn Fn(&Value) -> Value,
    sign: f64,
) -> EngineResult<(String, Value)> {
    let mag = render(params.get("magnitude").unwrap_or(&Value::Null));
    let unit = params
        .get("timeUnit")
        .and_then(Value::as_str)
        .unwrap_or("days");
    let duration = params
        .get("duration")
        .map(render)
        .and_then(|v| as_number_f64(&v))
        .unwrap_or(0.0);
    let dt = parse_dt(&mag, None, chrono_tz::UTC)?;
    let d = duration * sign;
    let shifted = match unit {
        "years" => add_months(dt, (d.trunc() * 12.0) as i64, chrono_tz::UTC)?,
        "quarters" => add_months(dt, (d.trunc() * 3.0) as i64, chrono_tz::UTC)?,
        "months" => add_months(dt, d.trunc() as i64, chrono_tz::UTC)?,
        "weeks" => shift_fixed(dt, d * 7.0 * 86_400_000.0)?,
        "days" => shift_fixed(dt, d * 86_400_000.0)?,
        "hours" => shift_fixed(dt, d * 3_600_000.0)?,
        "minutes" => shift_fixed(dt, d * 60_000.0)?,
        "seconds" => shift_fixed(dt, d * 1000.0)?,
        "milliseconds" => shift_fixed(dt, d)?,
        other => {
            return Err(EngineError::new(format!(
                "dateTime: timeUnit tak dikenal '{other}'"
            )))
        }
    };
    Ok((field(params, "newDate"), Value::String(out(&shifted))))
}

fn shift_fixed(dt: chrono::DateTime<Tz>, ms: f64) -> EngineResult<chrono::DateTime<Tz>> {
    let dur = chrono::Duration::milliseconds(ms as i64);
    dt.checked_add_signed(dur)
        .ok_or_else(|| EngineError::new("dateTime: hasil di luar rentang"))
}

/// Tambah bulan kalender dengan clamp akhir bulan (31 Jan + 1 → 28 Feb).
fn add_months(dt: chrono::DateTime<Tz>, months: i64, tz: Tz) -> EngineResult<chrono::DateTime<Tz>> {
    let naive = dt.naive_local();
    let total = naive.year() as i64 * 12 + naive.month0() as i64 + months;
    let y = total.div_euclid(12);
    let m = (total.rem_euclid(12) + 1) as u32;
    let dim = days_in_month(y as i32, m);
    let nd = chrono::NaiveDate::from_ymd_opt(y as i32, m, naive.day().min(dim))
        .and_then(|date| date.and_hms_opt(naive.hour(), naive.minute(), naive.second()))
        .and_then(|d| d.with_nanosecond(naive.nanosecond()))
        .ok_or_else(|| EngineError::new("dateTime: tanggal tak valid"))?;
    from_local(nd, tz)
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn op_format(
    params: &HashMap<String, Value>,
    render: &dyn Fn(&Value) -> Value,
) -> EngineResult<(String, Value)> {
    let date = render(params.get("date").unwrap_or(&Value::Null));
    let name = field(params, "formattedDate");
    if date.is_null() {
        return Ok((name, Value::Null));
    }
    let format = params
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("MM/dd/yyyy");
    let from_format = params
        .get("options")
        .and_then(|o| o.get("fromFormat"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty() && !s.starts_with("e.g"));
    let dt = parse_dt(&date, from_format, chrono_tz::UTC)?;
    let fmt = if format == "custom" {
        params
            .get("customFormat")
            .and_then(Value::as_str)
            .unwrap_or("")
    } else {
        format
    };
    if fmt == "x" {
        return Ok((name, Value::String(dt.timestamp_millis().to_string())));
    }
    let chrono_fmt = luxon_to_chrono(fmt)?;
    Ok((name, Value::String(dt.format(&chrono_fmt).to_string())))
}

fn op_round(
    params: &HashMap<String, Value>,
    render: &dyn Fn(&Value) -> Value,
) -> EngineResult<(String, Value)> {
    let date = render(params.get("date").unwrap_or(&Value::Null));
    let mode = params
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("roundDown");
    let (unit_key, def) = match mode {
        "roundDown" => ("toNearest", "month"),
        "roundUp" => ("to", "month"),
        other => {
            return Err(EngineError::new(format!(
                "dateTime: round mode tak dikenal '{other}'"
            )))
        }
    };
    let unit = params.get(unit_key).and_then(Value::as_str).unwrap_or(def);
    let dt = parse_dt(&date, None, chrono_tz::UTC)?;
    let result = if mode == "roundDown" {
        start_of(&dt, unit)?
    } else {
        let plus1 = match unit {
            "year" => add_months(dt, 12, chrono_tz::UTC)?,
            "quarter" => add_months(dt, 3, chrono_tz::UTC)?,
            "month" => add_months(dt, 1, chrono_tz::UTC)?,
            "week" => shift_fixed(dt, 7.0 * 86_400_000.0)?,
            "day" => shift_fixed(dt, 86_400_000.0)?,
            "hour" => shift_fixed(dt, 3_600_000.0)?,
            "minute" => shift_fixed(dt, 60_000.0)?,
            "second" => shift_fixed(dt, 1000.0)?,
            other => {
                return Err(EngineError::new(format!(
                    "dateTime: round unit tak dikenal '{other}'"
                )))
            }
        };
        start_of(&plus1, unit)?
    };
    Ok((field(params, "roundedDate"), Value::String(out(&result))))
}

fn start_of(dt: &chrono::DateTime<Tz>, unit: &str) -> EngineResult<chrono::DateTime<Tz>> {
    let n = dt.naive_local();
    let bad = || EngineError::new("dateTime: gagal membulatkan tanggal");
    let nd = match unit {
        "year" => chrono::NaiveDate::from_ymd_opt(n.year(), 1, 1)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .ok_or_else(bad)?,
        "quarter" => {
            let m = (n.month0() / 3) * 3 + 1;
            chrono::NaiveDate::from_ymd_opt(n.year(), m, 1)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .ok_or_else(bad)?
        }
        "month" => chrono::NaiveDate::from_ymd_opt(n.year(), n.month(), 1)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .ok_or_else(bad)?,
        "week" => (n.date() - chrono::Duration::days(n.weekday().num_days_from_monday() as i64))
            .and_hms_opt(0, 0, 0)
            .ok_or_else(bad)?,
        "day" => n.date().and_hms_opt(0, 0, 0).ok_or_else(bad)?,
        "hour" => n.date().and_hms_opt(n.hour(), 0, 0).ok_or_else(bad)?,
        "minute" => n
            .date()
            .and_hms_opt(n.hour(), n.minute(), 0)
            .ok_or_else(bad)?,
        "second" => n.with_nanosecond(0).ok_or_else(bad)?,
        other => {
            return Err(EngineError::new(format!(
                "dateTime: round unit tak dikenal '{other}'"
            )))
        }
    };
    from_local(nd, chrono_tz::UTC)
}

fn op_between(
    params: &HashMap<String, Value>,
    render: &dyn Fn(&Value) -> Value,
) -> EngineResult<(String, Value)> {
    let start = render(params.get("startDate").unwrap_or(&Value::Null));
    let end = render(params.get("endDate").unwrap_or(&Value::Null));
    let s = parse_dt(&start, None, chrono_tz::UTC)?;
    let e = parse_dt(&end, None, chrono_tz::UTC)?;
    let units: Vec<String> = match params.get("units") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(u)) => vec![u.clone()],
        _ => vec!["day".to_string()],
    };
    if units.is_empty() {
        return Err(EngineError::new("dateTime: units kosong"));
    }
    for u in &units {
        if matches!(u.as_str(), "month" | "year") {
            return Err(EngineError::new(
                "dateTime: unit 'month'/'year' belum didukung — pakai week/day/hour/minute/second/millisecond",
            ));
        }
        if !matches!(
            u.as_str(),
            "week" | "day" | "hour" | "minute" | "second" | "millisecond"
        ) {
            return Err(EngineError::new(format!(
                "dateTime: unit tak dikenal '{u}'"
            )));
        }
    }
    let total_ms = (e.timestamp_millis() - s.timestamp_millis()) as f64;
    let order = [
        ("week", 604800000.0),
        ("day", 86400000.0),
        ("hour", 3600000.0),
        ("minute", 60000.0),
        ("second", 1000.0),
        ("millisecond", 1.0),
    ];
    let req: Vec<(&str, f64)> = order
        .into_iter()
        .filter(|(u, _)| units.iter().any(|x| x == u))
        .collect();
    let mut rem = total_ms;
    let mut obj = Map::new();
    for (i, (u, div)) in req.iter().enumerate() {
        let last = i + 1 == req.len();
        let v = if last { rem / div } else { (rem / div).trunc() };
        rem -= v * div;
        obj.insert(u.to_string(), num_or_int(v));
    }
    let name = field(params, "timeDifference");
    let iso = params
        .get("options")
        .and_then(|o| o.get("isoString"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if iso {
        return Ok((name, Value::String(iso_duration(total_ms))));
    }
    Ok((name, Value::Object(obj)))
}

fn num_or_int(v: f64) -> Value {
    if v.fract() == 0.0 && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
        serde_json::json!(v as i64)
    } else {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

fn iso_duration(total_ms: f64) -> String {
    if total_ms == 0.0 {
        return "PT0S".to_string();
    }
    let neg = total_ms < 0.0;
    let mut rem = total_ms.abs().round() as i64;
    let w = rem / 604800000;
    rem %= 604800000;
    let d = rem / 86400000;
    rem %= 86400000;
    let h = rem / 3600000;
    rem %= 3600000;
    let m = rem / 60000;
    rem %= 60000;
    let s = rem / 1000;
    let ms = rem % 1000;
    let mut o = if neg {
        "-P".to_string()
    } else {
        "P".to_string()
    };
    if w > 0 {
        o.push_str(&format!("{w}W"));
    }
    if d > 0 {
        o.push_str(&format!("{d}D"));
    }
    if h > 0 || m > 0 || s > 0 || ms > 0 {
        o.push('T');
    }
    if h > 0 {
        o.push_str(&format!("{h}H"));
    }
    if m > 0 {
        o.push_str(&format!("{m}M"));
    }
    if s > 0 || ms > 0 {
        if ms > 0 {
            let inner = format!("{s}.{:03}", ms);
            o.push_str(inner.trim_end_matches('0').trim_end_matches('.'));
            o.push('S');
        } else {
            o.push_str(&format!("{s}S"));
        }
    }
    if o == "P" || o == "-P" {
        return "PT0S".to_string();
    }
    o
}

fn op_extract(
    params: &HashMap<String, Value>,
    render: &dyn Fn(&Value) -> Value,
) -> EngineResult<(String, Value)> {
    let date = render(params.get("date").unwrap_or(&Value::Null));
    let part = params
        .get("part")
        .and_then(Value::as_str)
        .unwrap_or("month");
    let dt = parse_dt(&date, None, chrono_tz::UTC)?;
    let n = dt.naive_local();
    let v: i64 = match part {
        "year" => n.year() as i64,
        "month" => n.month() as i64,
        "week" => n.iso_week().week() as i64,
        "day" => n.day() as i64,
        "hour" => n.hour() as i64,
        "minute" => n.minute() as i64,
        "second" => n.second() as i64,
        other => {
            return Err(EngineError::new(format!(
                "dateTime: part tak dikenal '{other}'"
            )))
        }
    };
    Ok((field(params, "datePart"), serde_json::json!(v)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ident(v: &Value) -> Value {
        v.clone()
    }

    fn p(pairs: Vec<(&str, Value)>) -> HashMap<String, Value> {
        pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    #[test]
    fn current_with_and_without_time() {
        let (name, v) =
            compute(&p(vec![("operation", json!("getCurrentDate"))]), &ident).expect("run");
        assert_eq!(name, "currentDate");
        let s = v.as_str().expect("string");
        assert!(s.contains('T') && s.contains("+00:00"), "{s}");
        let (_, v) = compute(
            &p(vec![
                ("operation", json!("getCurrentDate")),
                ("includeTime", json!(false)),
            ]),
            &ident,
        )
        .expect("run");
        assert!(
            v.as_str().expect("string").ends_with("T00:00:00.000+00:00"),
            "{v}"
        );
    }

    #[test]
    fn add_subtract_and_month_clamp() {
        let (name, v) = compute(
            &p(vec![
                ("operation", json!("addToDate")),
                ("magnitude", json!("2026-01-01T00:00:00+00:00")),
                ("timeUnit", json!("days")),
                ("duration", json!(10)),
            ]),
            &ident,
        )
        .expect("run");
        assert_eq!(name, "newDate");
        assert_eq!(v, json!("2026-01-11T00:00:00.000+00:00"));
        let (_, v) = compute(
            &p(vec![
                ("operation", json!("subtractFromDate")),
                ("magnitude", json!("2026-01-01T02:00:00+00:00")),
                ("timeUnit", json!("hours")),
                ("duration", json!(3)),
            ]),
            &ident,
        )
        .expect("run");
        assert_eq!(v, json!("2025-12-31T23:00:00.000+00:00"));
        let (_, v) = compute(
            &p(vec![
                ("operation", json!("addToDate")),
                ("magnitude", json!("2026-01-31")),
                ("timeUnit", json!("months")),
                ("duration", json!(1)),
            ]),
            &ident,
        )
        .expect("run");
        assert_eq!(v, json!("2026-02-28T00:00:00.000+00:00"));
    }

    #[test]
    fn format_presets_custom_and_unix() {
        let base = vec![
            ("operation", json!("formatDate")),
            ("date", json!("2026-09-11T14:30:00+00:00")),
        ];
        let (_, v) = compute(
            &p([base.clone(), vec![("format", json!("MM/dd/yyyy"))]].concat()),
            &ident,
        )
        .expect("run");
        assert_eq!(v, json!("09/11/2026"));
        let (_, v) = compute(
            &p([base.clone(), vec![("format", json!("X"))]].concat()),
            &ident,
        )
        .expect("run");
        assert_eq!(
            v,
            json!(
                chrono::DateTime::parse_from_rfc3339("2026-09-11T14:30:00+00:00")
                    .expect("parse")
                    .timestamp()
                    .to_string()
            )
        );
        let (_, v) = compute(
            &p([
                base.clone(),
                vec![
                    ("format", json!("custom")),
                    ("customFormat", json!("dd-MM-yyyy HH:mm")),
                ],
            ]
            .concat()),
            &ident,
        )
        .expect("run");
        assert_eq!(v, json!("11-09-2026 14:30"));
    }

    #[test]
    fn round_down_and_up() {
        let (_, v) = compute(
            &p(vec![
                ("operation", json!("roundDate")),
                ("date", json!("2026-09-11T14:30:45+00:00")),
                ("mode", json!("roundDown")),
                ("toNearest", json!("day")),
            ]),
            &ident,
        )
        .expect("run");
        assert_eq!(v, json!("2026-09-11T00:00:00.000+00:00"));
        let (_, v) = compute(
            &p(vec![
                ("operation", json!("roundDate")),
                ("date", json!("2026-09-11T14:30:45+00:00")),
                ("mode", json!("roundUp")),
                ("to", json!("month")),
            ]),
            &ident,
        )
        .expect("run");
        assert_eq!(v, json!("2026-10-01T00:00:00.000+00:00"));
    }

    #[test]
    fn between_days_and_iso() {
        let (_, v) = compute(
            &p(vec![
                ("operation", json!("getTimeBetweenDates")),
                ("startDate", json!("2026-09-01")),
                ("endDate", json!("2026-09-11")),
                ("units", json!(["day"])),
            ]),
            &ident,
        )
        .expect("run");
        assert_eq!(v, json!({"day": 10}));
        let (_, v) = compute(
            &p(vec![
                ("operation", json!("getTimeBetweenDates")),
                ("startDate", json!("2026-09-11T00:00:00+00:00")),
                ("endDate", json!("2026-09-11T01:30:00+00:00")),
                ("units", json!(["hour", "minute"])),
                ("options", json!({"isoString": true})),
            ]),
            &ident,
        )
        .expect("run");
        assert_eq!(v, json!("PT1H30M"));
        let err = compute(
            &p(vec![
                ("operation", json!("getTimeBetweenDates")),
                ("startDate", json!("2026-09-01")),
                ("endDate", json!("2026-09-11")),
                ("units", json!(["month"])),
            ]),
            &ident,
        )
        .expect_err("month diff");
        assert!(err.to_string().contains("belum didukung"), "{err}");
    }

    #[test]
    fn extract_parts() {
        let (_, v) = compute(
            &p(vec![
                ("operation", json!("extractDate")),
                ("date", json!("2026-09-11T14:30:00+00:00")),
                ("part", json!("month")),
            ]),
            &ident,
        )
        .expect("run");
        assert_eq!(v, json!(9));
    }

    #[test]
    fn invalid_date_and_from_format() {
        let err = compute(
            &p(vec![
                ("operation", json!("formatDate")),
                ("date", json!("besok pagi")),
                ("format", json!("yyyy-MM-dd")),
            ]),
            &ident,
        )
        .expect_err("must fail");
        assert!(err.to_string().contains("Invalid date format"), "{err}");
        let (_, v) = compute(
            &p(vec![
                ("operation", json!("formatDate")),
                ("date", json!("11/09/2026 14:30:00")),
                ("format", json!("yyyy-MM-dd")),
                ("options", json!({"fromFormat": "dd/MM/yyyy HH:mm:ss"})),
            ]),
            &ident,
        )
        .expect("run");
        assert_eq!(v, json!("2026-09-11"));
    }
}
