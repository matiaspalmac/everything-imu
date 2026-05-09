use persistence::PersistenceDb;
use std::sync::Arc;
use std::thread;
use tempfile::tempdir;

#[test]
fn concurrent_settings_writes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("c.db");
    let db = Arc::new(PersistenceDb::open(&path).unwrap());

    let mut handles = Vec::new();
    for thread_id in 0..4u8 {
        let db = db.clone();
        handles.push(thread::spawn(move || {
            for i in 0..250u32 {
                let key = format!("k_{thread_id}_{i}");
                let val = format!("v_{i}");
                db.set_setting(&key, &val).unwrap();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(db.get_setting("k_0_249").unwrap(), Some("v_249".into()));
    assert_eq!(db.get_setting("k_3_100").unwrap(), Some("v_100".into()));
}
