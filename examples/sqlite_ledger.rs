//! Writes expenses to a SQLite file, then reads the totals for each category
//! into a struct. A second, read-only connection reads next to the writer.
//!
//! ```sh
//! cargo run --example sqlite_ledger
//! ```
//!
//! The database is `target/smop-example-output/sqlite-ledger/ledger.db`.

use std::path::Path;

use smop::prelude::*;

/// The total of one category.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(crate = "smop::serde")]
struct Total {
    category: String,
    entries: u32,
    amount: f64,
}

const EXPENSES: [(&str, &str, f64); 5] = [
    ("2026-11-01", "food", 12.5),
    ("2026-11-02", "travel", 40.0),
    ("2026-11-03", "food", 7.25),
    ("2026-11-03", "books", 30.0),
    ("2026-11-04", "travel", 15.5),
];

fn main() -> Result<()> {
    let out = Path::new("target/smop-example-output/sqlite-ledger");
    fs::remove_if_exists(out)?;
    fs::create_dir_all(out)?;
    let file = out.join("ledger.db");

    let db = sqlite::open(&file)?;
    record(&db, &EXPENSES)?;

    let reader = sqlite::open_read_only(&file)?;
    for total in totals(&reader)? {
        println!(
            "{:<8} {:>2} entries {:>8.2}",
            total.category, total.entries, total.amount
        );
    }
    println!("Database: {}", file.display());
    Ok(())
}

/// Makes the table and adds each expense. Returns the number of rows added.
///
/// # Errors
///
/// Returns an error that quotes the SQL statement that failed.
fn record(db: &sqlite::Connection, expenses: &[(&str, &str, f64)]) -> Result<usize> {
    sqlite::execute(
        db,
        "CREATE TABLE IF NOT EXISTS expense (
            id INTEGER PRIMARY KEY,
            day TEXT NOT NULL,
            category TEXT NOT NULL,
            amount REAL NOT NULL CHECK (amount > 0)
        )",
        (),
    )?;
    let mut added = 0;
    for (day, category, amount) in expenses {
        added += sqlite::execute(
            db,
            "INSERT INTO expense (day, category, amount) VALUES (?1, ?2, ?3)",
            (day, category, amount),
        )?;
    }
    Ok(added)
}

/// The totals for each category, the largest amount first.
///
/// # Errors
///
/// Returns an error if the query fails.
fn totals(db: &sqlite::Connection) -> Result<Vec<Total>> {
    sqlite::query(
        db,
        "SELECT category, count(*) AS entries, sum(amount) AS amount
         FROM expense GROUP BY category ORDER BY amount DESC, category",
        (),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(category: &str, entries: u32, amount: f64) -> Total {
        Total {
            category: category.into(),
            entries,
            amount,
        }
    }

    #[test]
    fn totals_for_each_category() {
        let db = sqlite::open(":memory:").unwrap();
        assert_eq!(record(&db, &EXPENSES).unwrap(), 5);
        assert_eq!(
            totals(&db).unwrap(),
            [
                total("travel", 2, 55.5),
                total("books", 1, 30.0),
                total("food", 2, 19.75),
            ]
        );
    }

    #[test]
    fn no_expenses_no_totals() {
        let db = sqlite::open(":memory:").unwrap();
        assert_eq!(record(&db, &[]).unwrap(), 0);
        assert!(totals(&db).unwrap().is_empty());
    }

    #[test]
    fn a_bad_amount_quotes_the_statement() {
        let db = sqlite::open(":memory:").unwrap();
        let err = record(&db, &[("2026-11-05", "food", -1.0)]).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.starts_with("SQL error in \"INSERT INTO expense"),
            "{msg}"
        );
        assert!(msg.contains("CHECK constraint failed"), "{msg}");
    }

    #[test]
    fn the_reader_sees_the_writer() {
        let dir = fs::temp_dir().unwrap();
        let file = dir.path().join("ledger.db");
        let db = sqlite::open(&file).unwrap();
        record(&db, &EXPENSES[..2]).unwrap();
        let reader = sqlite::open_read_only(&file).unwrap();
        assert_eq!(totals(&reader).unwrap().len(), 2);
        record(&db, &EXPENSES[2..]).unwrap();
        assert_eq!(totals(&reader).unwrap().len(), 3);
    }
}
