//! Profiling harness: `valgrind --tool=callgrind target/release/examples/bolt_sqlite`.
use smop::prelude::*;

#[derive(Deserialize)]
#[serde(crate = "smop::serde")]
#[allow(dead_code)]
struct Row {
    id: i64,
    name: String,
    score: f64,
    done: bool,
    note: Option<String>,
}

fn main() -> Result<()> {
    let db = sqlite::open(":memory:")?;
    sqlite::execute(
        &db,
        "CREATE TABLE t (id INTEGER, name TEXT, score REAL, done INTEGER, note TEXT)",
        (),
    )?;
    db.execute_batch("BEGIN")?;
    for i in 0..20_000i64 {
        sqlite::execute(
            &db,
            "INSERT INTO t VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                i,
                format!("name-{i}"),
                f64::from(i32::try_from(i)?) / 3.0,
                i % 2 == 0,
                if i % 3 == 0 {
                    None
                } else {
                    Some("some note text")
                },
            ),
        )?;
    }
    db.execute_batch("CREATE INDEX t_id ON t (id); COMMIT")?;
    let mut n = 0;
    for _ in 0..5 {
        let rows: Vec<Row> = sqlite::query(&db, "SELECT * FROM t", ())?;
        n += rows.len();
        let pairs: Vec<(i64, String)> = sqlite::query(&db, "SELECT id, name FROM t", ())?;
        n += pairs.len();
        for i in 0..500i64 {
            let r: Option<Row> = sqlite::query_opt(&db, "SELECT * FROM t WHERE id = ?1", [i])?;
            n += usize::from(r.is_some());
        }
    }
    println!("{n}");
    Ok(())
}
