//! Profiling harness: `valgrind --tool=callgrind target/release/examples/bolt_sqlite`.
use smop::prelude::*;

#[derive(Deserialize)]
#[serde(crate = "smop::serde")]
#[allow(dead_code)]
struct Task {
    id: i64,
    title: String,
    score: f64,
    done: bool,
    note: Option<String>,
}

fn main() -> Result<()> {
    let db = sqlite::open(":memory:")?;
    sqlite::execute(
        &db,
        "CREATE TABLE task (id INTEGER PRIMARY KEY, title TEXT, score REAL, done INTEGER, note TEXT)",
        (),
    )?;
    db.execute_batch("BEGIN")?;
    for i in 0..20_000 {
        sqlite::execute(
            &db,
            "INSERT INTO task (title, score, done, note) VALUES (?1, ?2, ?3, ?4)",
            (format!("task number {i}"), i as f64 / 7.0, i % 3 == 0, (i % 5 == 0).then(|| "note")),
        )?;
    }
    db.execute_batch("COMMIT")?;
    let mut n = 0;
    for _ in 0..5 {
        let rows: Vec<Task> = sqlite::query(&db, "SELECT * FROM task", ())?;
        n += rows.len();
        let pairs: Vec<(i64, String)> = sqlite::query(&db, "SELECT id, title FROM task", ())?;
        n += pairs.len();
    }
    println!("{n} rows");
    Ok(())
}
