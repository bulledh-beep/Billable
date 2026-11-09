//! Behaviour the Node parity harness can't drive: avatars (Electron's
//! `nativeImage`), import/export and profile switching (from `ipc.ts`), and
//! the timer's elapsed-time formatting.

use std::fs;
use std::path::PathBuf;

use billable_core::timer::{elapsed_seconds, format_elapsed, format_elapsed_readable};
use billable_core::*;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("billable-core-api-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// An uncompressed 24-bit BMP, which `sips` reads.
fn write_bmp(path: &std::path::Path, w: u32, h: u32) {
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut b = Vec::with_capacity(size as usize);
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&size.to_le_bytes());
    b.extend_from_slice(&[0; 4]);
    b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&(w as i32).to_le_bytes());
    b.extend_from_slice(&(h as i32).to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&[0; 24]);
    for y in 0..h {
        for x in 0..w {
            b.extend_from_slice(&[(x % 256) as u8, (y % 256) as u8, 128]);
        }
        b.resize(b.len() + (row - w * 3) as usize, 0);
    }
    fs::write(path, b).unwrap();
}

fn base64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let clean: Vec<u8> = s.bytes().filter(|c| *c != b'=').collect();
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, c)| n | (val(*c) as u32) << (18 - 6 * i));
        for i in 0..chunk.len().saturating_sub(1) {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    out
}

fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    let be = |i: usize| u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]);
    (be(16), be(20))
}

#[test]
fn avatars_are_scaled_pngs() {
    let dir = temp_dir("avatar");
    let profiles = Profiles::new(dir.join("data"), Clock::fixed(0));
    profiles.migrate_legacy_if_needed().unwrap();

    let big = dir.join("big.bmp");
    write_bmp(&big, 600, 300);
    let p = profiles.set_profile_avatar("default", &big).unwrap();
    assert_eq!(png_size(&base64_decode(p.avatar.as_deref().unwrap())), (256, 128));

    let small = dir.join("small.bmp");
    write_bmp(&small, 100, 37);
    let p = profiles.set_profile_avatar("default", &small).unwrap();
    assert_eq!(png_size(&base64_decode(p.avatar.as_deref().unwrap())), (100, 37));
    assert!(fs::read_to_string(profiles.index_path()).unwrap().contains("\"avatar\": \"iVBORw0KGgo"));

    let junk = dir.join("junk.png");
    fs::write(&junk, b"not an image").unwrap();
    let err = profiles.set_profile_avatar("default", &junk).unwrap_err().to_string();
    assert_eq!(err, "Could not read image — unsupported format or corrupt file");
    let err = profiles.set_profile_avatar("default", dir.join("missing.png")).unwrap_err().to_string();
    assert!(err.starts_with("Image not found: "), "{err}");
    let err = profiles.set_profile_avatar("nope", &small).unwrap_err().to_string();
    assert_eq!(err, "Profile nope not found");

    let p = profiles.clear_profile_avatar("default").unwrap();
    assert!(p.avatar.is_none());
    assert!(!fs::read_to_string(profiles.index_path()).unwrap().contains("avatar"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn export_and_import() {
    let dir = temp_dir("import");
    let clock = Clock::fixed_at("2026-11-05T18:30:00.000Z").unwrap();
    let a = Db::open(dir.join("a.db"), clock.clone()).unwrap();
    a.create_client(&ClientInput { name: "Exported".into(), ..Default::default() }).unwrap();
    let file = dir.join("export.db");
    fs::write(&file, b"old").unwrap();
    a.export_to(&file).unwrap();
    drop(a);

    let b = Db::open(dir.join("b.db"), clock.clone()).unwrap();
    b.create_client(&ClientInput { name: "Replaced".into(), ..Default::default() }).unwrap();
    let junk = dir.join("junk.db");
    fs::write(&junk, b"definitely not sqlite").unwrap();
    let b = match b.import_from(&junk) {
        Ok(_) => panic!("imported junk"),
        Err(e) => {
            assert_eq!(e.to_string(), "That file is not a Billable database");
            Db::open(dir.join("b.db"), clock.clone()).unwrap()
        }
    };
    let backup = b.import_from(&file).unwrap();
    assert!(backup.file_name().unwrap().to_string_lossy().starts_with("billable-backup-before-import-2026-11-05T18-30-00"));
    let b = Db::open(dir.join("b.db"), clock).unwrap();
    let names: Vec<String> = b.list_clients().unwrap().into_iter().map(|c| c.name.clone()).collect();
    assert_eq!(names, ["Exported"]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn switching_profiles_stops_the_timer() {
    let dir = temp_dir("switch");
    let clock = Clock::fixed_at("2026-11-05T18:30:00.000Z").unwrap();
    let profiles = Profiles::new(dir.join("data"), clock.clone());
    profiles.migrate_legacy_if_needed().unwrap();
    let other = profiles.create_profile("Other", None).unwrap();

    let mut db = Db::open_profile(&profiles, "default", clock.clone()).unwrap();
    let c = db.create_client(&ClientInput { name: "C".into(), ..Default::default() }).unwrap();
    let p = db.create_project(&ProjectInput { client_id: c.id, name: "P".into(), ..Default::default() }).unwrap();
    let mut timer = TimerManager::new(&db).unwrap();
    let started = timer.start(&db, p.id, Some("work"), None).unwrap();
    clock.advance_ms(5 * 60_000);

    let active = profiles.switch_profile(&other.id, &mut db, &mut timer).unwrap().unwrap();
    assert_eq!(active.id, other.id);
    assert!(timer.active().is_none() && timer.paused().is_none());
    assert!(db.path().ends_with("profiles/other/billable.db"));
    assert!(db.list_clients().unwrap().is_empty());

    let default_db = Db::open_profile(&profiles, "default", clock).unwrap();
    let e = default_db.get_time_entry(started.id).unwrap().unwrap();
    assert_eq!((e.end_time.as_deref(), e.duration_minutes), (Some("2026-11-05T18:35:00.000Z"), 5.0));
    assert!(profiles.switch_profile("nope", &mut db, &mut timer).is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn elapsed_time_formatting() {
    let mut e = TimeEntry {
        id: 1,
        project_id: 1,
        description: String::new(),
        start_time: "2026-11-05T18:00:00.000Z".into(),
        end_time: None,
        duration_minutes: 61.5,
        paused_at: Some("2026-11-05T19:00:00.000Z".into()),
        active_since: None,
        is_billable: 1,
        is_invoiced: 0,
        invoice_id: None,
        created_at: String::new(),
        extra: Default::default(),
    };
    let now = billable_core::time::js_date_parse("2026-11-05T20:00:10.000Z") as i64;
    assert_eq!(format_elapsed(&e, now), "1:01:30");
    assert_eq!(format_elapsed_readable(&e, now), "1h 01m 30s");
    e.active_since = Some("2026-11-05T20:00:00.000Z".into());
    assert_eq!(elapsed_seconds(&e, now), 3700);
    e.active_since = Some(String::new()); // falls back to start_time
    e.duration_minutes = 0.0;
    assert_eq!(format_elapsed(&e, now), "2:00:10");
    e.duration_minutes = 0.25;
    e.active_since = None;
    assert_eq!(format_elapsed_readable(&e, now), "0m 15s");
}

#[test]
fn data_folder_honours_the_env_hook() {
    std::env::set_var("BILLABLE_USER_DATA", "/tmp/somewhere-else");
    assert_eq!(user_data_dir(), PathBuf::from("/tmp/somewhere-else"));
    std::env::remove_var("BILLABLE_USER_DATA");
    assert!(user_data_dir().ends_with("Library/Application Support/Billable"));
}
