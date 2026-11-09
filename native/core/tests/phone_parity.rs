//! Phone sync parity with the Electron app: replays
//! `parity/scenarios/phone/phone.json` through `billable_core::phone` and
//! compares every step with what `parity/node/phone.mjs` recorded from
//! phone-snapshot.ts / phone-actions.ts. Snapshots must match byte for byte
//! (they are Content HQ's contract). Skipped when the Node results are missing.

use std::fs;
use std::path::{Path, PathBuf};

use billable_core::phone::{apply_incoming, build_snapshot, list_activity, mark_acked, unacked_acks};
use billable_core::time::js_date_parse;
use billable_core::{Clock, Db, TimerManager};
use rusqlite::types::Value as SqlValue;
use serde_json::{json, Value};

fn parity_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("parity")
}

fn rows_json(db: &Db, sql: &str) -> Value {
    let mut stmt = db.connection().prepare(sql).unwrap();
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt
        .query_map([], |r| {
            let mut o = serde_json::Map::new();
            for (i, n) in names.iter().enumerate() {
                let v = match r.get::<_, SqlValue>(i)? {
                    SqlValue::Null => Value::Null,
                    SqlValue::Integer(i) => json!(i),
                    SqlValue::Real(f) => json!(f),
                    SqlValue::Text(s) => json!(s),
                    SqlValue::Blob(_) => Value::Null,
                };
                o.insert(n.clone(), v);
            }
            Ok(Value::Object(o))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    Value::Array(rows)
}

/// Numbers compare by value (5 = 5.0).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w))),
        _ => a == b,
    }
}

#[test]
fn phone_sync_matches_the_typescript() {
    let sc: Value = serde_json::from_str(&fs::read_to_string(parity_dir().join("scenarios/phone/phone.json")).unwrap()).unwrap();
    let results_path = parity_dir().join("out/phone/results.json");
    let Ok(text) = fs::read_to_string(&results_path) else {
        eprintln!("phone parity skipped: run `node phone.mjs` in parity/node first");
        return;
    };
    let expected: Vec<Value> = serde_json::from_str(&text).unwrap();
    std::env::set_var("TZ", sc["tz"].as_str().unwrap());

    let dir = std::env::temp_dir().join(format!("billable-phone-parity-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let clock = Clock::fixed_at(sc["now"].as_str().unwrap()).unwrap();
    let db = Db::open(dir.join("billable.db"), clock.clone()).unwrap();
    for s in sc["seed"].as_array().unwrap() {
        db.connection().execute_batch(s.as_str().unwrap()).unwrap();
    }
    let mut timer = TimerManager::new(&db).unwrap();

    let steps = sc["steps"].as_array().unwrap();
    assert_eq!(steps.len(), expected.len(), "step count");
    let mut failures = vec![];
    for (i, step) in steps.iter().enumerate() {
        let op = step[0].as_str().unwrap();
        let got: Value = match op {
            "snapshot" => Value::String(serde_json::to_string(&build_snapshot(&db, &timer).unwrap()).unwrap()),
            "batch" => {
                clock.set_ms(js_date_parse(step[1].as_str().unwrap()) as i64);
                let list = step[2].as_array().unwrap();
                serde_json::to_value(apply_incoming(&db, &mut timer, list).unwrap()).unwrap()
            }
            "acks" => serde_json::to_value(unacked_acks(&db).unwrap()).unwrap(),
            "ack" => {
                let ids: Vec<String> = step[1].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
                mark_acked(&db, &ids).unwrap();
                Value::Null
            }
            "activity" => serde_json::to_value(list_activity(&db, step[1].as_i64().unwrap())).unwrap(),
            "sql" => rows_json(&db, step[1].as_str().unwrap()),
            other => panic!("unknown op {other}"),
        };
        let want = &expected[i]["value"];
        if !same(&got, want) {
            failures.push(format!("step {i} ({op}):\n  rust: {got}\n  node: {want}"));
        }
    }
    let _ = fs::remove_dir_all(&dir);
    assert!(failures.is_empty(), "{} step(s) differ:\n{}", failures.len(), failures.join("\n"));
}
