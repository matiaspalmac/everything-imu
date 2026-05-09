use persistence::PersistenceDb;
use tempfile::tempdir;

#[test]
fn open_creates_schema() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let _db = PersistenceDb::open(&path).unwrap();
    let _db2 = PersistenceDb::open(&path).unwrap();
}

#[test]
fn open_in_memory_works() {
    let _db = PersistenceDb::open_in_memory().unwrap();
}
