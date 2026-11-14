//! `sqlite` feature: open, execute, query, row mapping and errors.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::collections::BTreeMap;
use std::path::Path;

use proptest::prelude::*;
use serde::Deserialize;
use smop::sqlite::{self, Connection, named_params, params};

fn memory() -> Connection {
    let db = sqlite::open(":memory:").unwrap();
    sqlite::execute(
        &db,
        "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, \
         age INTEGER, score REAL, active INTEGER, photo BLOB, role TEXT)",
        (),
    )
    .unwrap();
    db
}

fn add(db: &Connection, name: &str, age: Option<i64>) {
    sqlite::execute(
        db,
        "INSERT INTO person (name, age, score, active, photo, role) \
         VALUES (?1, ?2, 1.5, 1, x'0102', 'admin')",
        params![name, age],
    )
    .unwrap();
}

fn err<T: std::fmt::Debug>(r: anyhow::Result<T>) -> String {
    format!("{:#}", r.unwrap_err())
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Role {
    Admin,
    User,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Person {
    id: i64,
    name: String,
    age: Option<u32>,
    score: f64,
    active: bool,
    photo: Vec<u8>,
    role: Role,
}

// ---------------------------------------------------------------------------
// open and execute
// ---------------------------------------------------------------------------

#[test]
fn open_and_execute_on_a_temporary_database() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("app.db");
    {
        let db = sqlite::open(&file).unwrap();
        let n = sqlite::execute(&db, "CREATE TABLE t (x INTEGER)", ()).unwrap();
        assert_eq!(n, 0);
        let n = sqlite::execute(&db, "INSERT INTO t VALUES (1), (2), (3)", ()).unwrap();
        assert_eq!(n, 3);
        let n = sqlite::execute(&db, "UPDATE t SET x = x * 10 WHERE x > ?1", [1]).unwrap();
        assert_eq!(n, 2);
    }
    assert!(file.is_file());
    let db = sqlite::open(&file).unwrap();
    let xs: Vec<i64> = sqlite::query(&db, "SELECT x FROM t ORDER BY x", ()).unwrap();
    assert_eq!(xs, [1, 20, 30]);
}

#[test]
fn open_memory_database() {
    let db = sqlite::open(":memory:").unwrap();
    let one: i64 = sqlite::query_one(&db, "SELECT 1", ()).unwrap();
    assert_eq!(one, 1);
}

#[test]
fn open_error_names_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("missing-dir").join("app.db");
    let msg = err(sqlite::open(&file));
    assert!(msg.contains("Failed to open SQLite database"), "{msg}");
    assert!(msg.contains(&file.display().to_string()), "{msg}");
    assert!(!file.parent().unwrap().exists(), "no parent is made");
}

#[test]
fn execute_named_params() {
    let db = memory();
    let n = sqlite::execute(
        &db,
        "INSERT INTO person (name, age) VALUES (:name, :age)",
        named_params! { ":name": "ann", ":age": 30 },
    )
    .unwrap();
    assert_eq!(n, 1);
    let age: u32 = sqlite::query_one(&db, "SELECT age FROM person", ()).unwrap();
    assert_eq!(age, 30);
}

// ---------------------------------------------------------------------------
// open_read_only
// ---------------------------------------------------------------------------

#[test]
fn read_only_reads_next_to_a_writer() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("live.db");
    let writer = sqlite::open(&file).unwrap();
    sqlite::execute(&writer, "CREATE TABLE t (x INTEGER)", ()).unwrap();
    sqlite::execute(&writer, "INSERT INTO t VALUES (7)", ()).unwrap();

    let reader = sqlite::open_read_only(&file).unwrap();
    let x: i64 = sqlite::query_one(&reader, "SELECT x FROM t", ()).unwrap();
    assert_eq!(x, 7);

    sqlite::execute(&writer, "INSERT INTO t VALUES (8)", ()).unwrap();
    let n: i64 = sqlite::query_one(&reader, "SELECT count(*) FROM t", ()).unwrap();
    assert_eq!(n, 2);

    let msg = err(sqlite::execute(&reader, "INSERT INTO t VALUES (9)", ()));
    assert!(msg.contains("readonly"), "{msg}");
    assert!(msg.contains("\"INSERT INTO t VALUES (9)\""), "{msg}");
}

#[test]
fn read_only_does_not_make_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("none.db");
    let msg = err(sqlite::open_read_only(&file));
    assert!(msg.contains(&file.display().to_string()), "{msg}");
    assert!(!file.exists());
}

// ---------------------------------------------------------------------------
// query: row mapping
// ---------------------------------------------------------------------------

#[test]
fn query_rows_into_a_struct() {
    let db = memory();
    add(&db, "ann", Some(30));
    add(&db, "bob", None);
    let people: Vec<Person> = sqlite::query(&db, "SELECT * FROM person ORDER BY id", ()).unwrap();
    assert_eq!(
        people,
        [
            Person {
                id: 1,
                name: "ann".into(),
                age: Some(30),
                score: 1.5,
                active: true,
                photo: vec![1, 2],
                role: Role::Admin,
            },
            Person {
                id: 2,
                name: "bob".into(),
                age: None,
                score: 1.5,
                active: true,
                photo: vec![1, 2],
                role: Role::Admin,
            },
        ]
    );
}

#[test]
fn query_extra_columns_are_ignored_and_aliases_name_fields() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Short {
        who: String,
    }
    let db = memory();
    add(&db, "ann", Some(1));
    let rows: Vec<Short> =
        sqlite::query(&db, "SELECT id, name AS who, age FROM person", ()).unwrap();
    assert_eq!(rows, [Short { who: "ann".into() }]);
}

#[test]
fn query_tuples_scalars_and_options() {
    let db = memory();
    add(&db, "ann", Some(30));
    add(&db, "bob", None);

    let pairs: Vec<(String, Option<i64>)> =
        sqlite::query(&db, "SELECT name, age FROM person ORDER BY id", ()).unwrap();
    assert_eq!(pairs, [("ann".into(), Some(30)), ("bob".into(), None)]);

    let names: Vec<String> =
        sqlite::query(&db, "SELECT name FROM person ORDER BY name DESC", ()).unwrap();
    assert_eq!(names, ["bob", "ann"]);

    let max: Option<i64> = sqlite::query_one(&db, "SELECT max(age) FROM person", ()).unwrap();
    assert_eq!(max, Some(30));
    let none: Option<i64> =
        sqlite::query_one(&db, "SELECT max(age) FROM person WHERE id > 99", ()).unwrap();
    assert_eq!(none, None);
}

#[test]
fn query_into_a_map() {
    let db = memory();
    add(&db, "ann", Some(30));
    let rows: Vec<BTreeMap<String, Option<String>>> =
        sqlite::query(&db, "SELECT name, role, NULL AS gone FROM person", ()).unwrap();
    let mut want = BTreeMap::new();
    want.insert("name".to_string(), Some("ann".to_string()));
    want.insert("role".to_string(), Some("admin".to_string()));
    want.insert("gone".to_string(), None);
    assert_eq!(rows, [want]);
}

#[cfg(feature = "json")]
#[test]
fn query_into_json_values() {
    let db = memory();
    add(&db, "ann", Some(30));
    let rows: Vec<smop::serde_json::Value> =
        sqlite::query(&db, "SELECT name, age, score, NULL AS n FROM person", ()).unwrap();
    assert_eq!(
        rows,
        [smop::serde_json::json!({"name": "ann", "age": 30, "score": 1.5, "n": null})]
    );
}

#[test]
fn query_newtype_and_char_and_float_from_integer() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Id(i64);
    let db = sqlite::open(":memory:").unwrap();
    let id: Id = sqlite::query_one(&db, "SELECT 5", ()).unwrap();
    assert_eq!(id, Id(5));
    let c: char = sqlite::query_one(&db, "SELECT 'x'", ()).unwrap();
    assert_eq!(c, 'x');
    let f: f64 = sqlite::query_one(&db, "SELECT 2", ()).unwrap();
    assert_eq!(f, 2.0);
    let b: (bool, bool) = sqlite::query_one(&db, "SELECT 0, 5", ()).unwrap();
    assert_eq!(b, (false, true));
}

#[test]
fn query_runs_insert_returning() {
    let db = memory();
    let ids: Vec<i64> = sqlite::query(
        &db,
        "INSERT INTO person (name) VALUES (?1), (?2) RETURNING id",
        ["a", "b"],
    )
    .unwrap();
    assert_eq!(ids, [1, 2]);
}

#[test]
fn query_with_no_rows_is_empty() {
    let db = memory();
    let rows: Vec<Person> = sqlite::query(&db, "SELECT * FROM person", ()).unwrap();
    assert!(rows.is_empty());
}

#[test]
fn query_one_takes_the_first_row() {
    let db = memory();
    add(&db, "ann", None);
    add(&db, "bob", None);
    let name: String = sqlite::query_one(&db, "SELECT name FROM person ORDER BY id", ()).unwrap();
    assert_eq!(name, "ann");
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[test]
fn syntax_error_quotes_the_statement() {
    let db = memory();
    let msg = err(sqlite::query::<i64, _>(&db, "SELEC 1", ()));
    assert!(msg.starts_with("SQL error in \"SELEC 1\""), "{msg}");
    assert!(msg.contains("syntax error"), "{msg}");

    let msg = err(sqlite::execute(&db, "  DELETE FROM nope\n", ()));
    assert!(
        msg.starts_with("SQL error in \"DELETE FROM nope\""),
        "{msg}"
    );
    assert!(msg.contains("no such table: nope"), "{msg}");
}

#[test]
fn constraint_error_quotes_the_statement_but_not_the_values() {
    let db = memory();
    add(&db, "ann", None);
    let sql = "INSERT INTO person (name, age) VALUES (?1, ?2)";
    let msg = err(sqlite::execute(&db, sql, params!["ann", 424_242]));
    assert!(msg.starts_with(&format!("SQL error in {sql:?}")), "{msg}");
    assert!(
        msg.contains("UNIQUE constraint failed: person.name"),
        "{msg}"
    );
    assert!(
        !msg.contains("424242"),
        "a parameter value is in the error: {msg}"
    );
}

#[test]
fn the_rusqlite_error_is_the_cause() {
    let db = memory();
    add(&db, "ann", None);
    let e = sqlite::execute(&db, "INSERT INTO person (name) VALUES ('ann')", ()).unwrap_err();
    let cause = e
        .downcast_ref::<smop::rusqlite::Error>()
        .expect("rusqlite::Error in the chain");
    assert_eq!(
        cause.sqlite_error_code(),
        Some(smop::rusqlite::ErrorCode::ConstraintViolation)
    );
}

#[test]
fn execute_rejects_two_statements() {
    let db = memory();
    let msg = err(sqlite::execute(
        &db,
        "DELETE FROM person; DELETE FROM person",
        (),
    ));
    assert!(msg.starts_with("SQL error in"), "{msg}");
    assert!(
        msg.contains("DELETE FROM person; DELETE FROM person"),
        "{msg}"
    );
}

#[test]
fn execute_with_wrong_parameter_count_quotes_the_statement() {
    let db = memory();
    let msg = err(sqlite::execute(
        &db,
        "INSERT INTO person (name) VALUES (?1)",
        (),
    ));
    assert!(msg.starts_with("SQL error in"), "{msg}");
}

#[test]
fn null_into_a_non_option_names_row_and_column() {
    let db = memory();
    add(&db, "ann", Some(1));
    add(&db, "bob", None);
    let sql = "SELECT name, age FROM person ORDER BY id";
    let msg = err(sqlite::query::<(String, u32), _>(&db, sql, ()));
    assert!(
        msg.starts_with(&format!("Failed to read row 2 of {sql:?}")),
        "{msg}"
    );
    assert!(msg.contains("column \"age\""), "{msg}");
    assert!(msg.contains("NULL"), "{msg}");
}

#[test]
fn null_into_a_struct_field() {
    #[derive(Debug, Deserialize)]
    struct Age {
        #[allow(dead_code)]
        age: u32,
    }
    let db = memory();
    add(&db, "ann", None);
    let msg = err(sqlite::query::<Age, _>(&db, "SELECT age FROM person", ()));
    assert!(msg.contains("row 1"), "{msg}");
    assert!(
        msg.contains("column \"age\": invalid type: NULL, expected u32"),
        "{msg}"
    );
}

#[test]
fn out_of_range_integer_is_an_error() {
    let db = sqlite::open(":memory:").unwrap();
    let msg = err(sqlite::query_one::<u8, _>(&db, "SELECT 300 AS big", ()));
    assert!(msg.contains("column \"big\""), "{msg}");
    assert!(msg.contains("300"), "{msg}");
    let msg = err(sqlite::query_one::<u64, _>(&db, "SELECT -1", ()));
    assert!(msg.contains("-1"), "{msg}");
}

#[test]
fn wrong_types_are_errors() {
    let db = sqlite::open(":memory:").unwrap();
    assert!(err(sqlite::query_one::<i64, _>(&db, "SELECT 'x'", ())).contains("expected i64"));
    assert!(err(sqlite::query_one::<i64, _>(&db, "SELECT 1.5", ())).contains("expected i64"));
    assert!(
        err(sqlite::query_one::<bool, _>(&db, "SELECT 'yes'", ())).contains("expected a boolean")
    );
    assert!(
        err(sqlite::query_one::<String, _>(&db, "SELECT x'00'", ())).contains("expected a string")
    );
    assert!(
        err(sqlite::query_one::<Role, _>(&db, "SELECT 'root'", ()))
            .contains("unknown variant `root`")
    );
}

#[test]
fn missing_column_names_the_field() {
    let db = memory();
    add(&db, "ann", None);
    let msg = err(sqlite::query::<Person, _>(
        &db,
        "SELECT id, name FROM person",
        (),
    ));
    // `age` is an `Option`: a missing column is `None`.
    assert!(msg.contains("missing field `score`"), "{msg}");
}

#[test]
fn column_count_must_match() {
    let db = sqlite::open(":memory:").unwrap();
    let msg = err(sqlite::query_one::<i64, _>(&db, "SELECT 1, 2", ()));
    assert!(msg.contains("2 columns"), "{msg}");
    let msg = err(sqlite::query_one::<(i64, i64), _>(
        &db,
        "SELECT 1, 2, 3",
        (),
    ));
    assert!(msg.contains("3 columns"), "{msg}");
    let msg = err(sqlite::query_one::<(i64, i64, i64), _>(
        &db,
        "SELECT 1, 2",
        (),
    ));
    assert!(msg.contains("2 columns"), "{msg}");
}

#[test]
fn duplicate_column_names_are_an_error_for_a_struct() {
    #[derive(Debug, Deserialize)]
    struct Pair {
        #[allow(dead_code)]
        id: i64,
    }
    let db = sqlite::open(":memory:").unwrap();
    let msg = err(sqlite::query_one::<Pair, _>(
        &db,
        "SELECT 1 AS id, 2 AS id",
        (),
    ));
    assert!(msg.contains("duplicate field `id`"), "{msg}");
}

#[test]
fn query_one_without_rows_quotes_the_statement() {
    let db = memory();
    let msg = err(sqlite::query_one::<i64, _>(
        &db,
        "SELECT id FROM person",
        (),
    ));
    assert_eq!(msg, "No rows from \"SELECT id FROM person\"");
}

#[test]
fn rusqlite_is_re_exported() {
    let db = smop::rusqlite::Connection::open_in_memory().unwrap();
    let n: i64 = sqlite::query_one(&db, "SELECT 41 + 1", ()).unwrap();
    assert_eq!(n, 42);
}

#[test]
fn transaction_derefs_to_a_connection() {
    let mut db = memory();
    let tx = db.transaction().unwrap();
    sqlite::execute(&tx, "INSERT INTO person (name) VALUES ('t')", ()).unwrap();
    tx.rollback().unwrap();
    let n: i64 = sqlite::query_one(&db, "SELECT count(*) FROM person", ()).unwrap();
    assert_eq!(n, 0);
}

fn _accepts_path(p: &Path) -> anyhow::Result<Connection> {
    sqlite::open(p)
}

// ---------------------------------------------------------------------------
// Property: values go in with params and come back the same
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, PartialEq)]
struct Row {
    i: i64,
    t: String,
    o: Option<i64>,
    f: f64,
    b: Vec<u8>,
}

proptest! {
    #[test]
    fn values_round_trip(
        i in any::<i64>(),
        t in ".*",
        o in proptest::option::of(any::<i64>()),
        f in proptest::num::f64::NORMAL | proptest::num::f64::ZERO,
        b in proptest::collection::vec(any::<u8>(), 0..64),
    ) {
        let db = sqlite::open(":memory:").unwrap();
        sqlite::execute(&db, "CREATE TABLE r (i INTEGER, t TEXT, o INTEGER, f REAL, b BLOB)", ()).unwrap();
        sqlite::execute(&db, "INSERT INTO r VALUES (?1, ?2, ?3, ?4, ?5)", params![i, t, o, f, b]).unwrap();
        let got: Row = sqlite::query_one(&db, "SELECT * FROM r", ()).unwrap();
        prop_assert_eq!(got, Row { i, t, o, f, b });
    }
}

#[test]
fn open_a_file_that_is_not_a_database_names_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    std::fs::write(
        &file,
        "this is not a database, it is a long text file .....",
    )
    .unwrap();
    for msg in [err(sqlite::open(&file)), err(sqlite::open_read_only(&file))] {
        assert!(msg.contains(&file.display().to_string()), "{msg}");
        assert!(msg.contains("not a database"), "{msg}");
    }
}

#[test]
fn blob_and_text_do_not_mix() {
    let db = sqlite::open(":memory:").unwrap();
    let msg = err(sqlite::query_one::<String, _>(
        &db,
        "SELECT x'6869' AS b",
        (),
    ));
    assert!(
        msg.contains("column \"b\": invalid type: byte array"),
        "{msg}"
    );
    let msg = err(sqlite::query_one::<char, _>(&db, "SELECT x'68'", ()));
    assert!(msg.contains("byte array"), "{msg}");
    let msg = err(sqlite::query_one::<Vec<u8>, _>(&db, "SELECT 'hi' AS t", ()));
    assert!(msg.contains("column \"t\""), "{msg}");
}

#[test]
fn a_row_as_a_vec() {
    let db = sqlite::open(":memory:").unwrap();
    let blob: Vec<u8> = sqlite::query_one(&db, "SELECT x'0a0b'", ()).unwrap();
    assert_eq!(blob, [10, 11]);
    let cols: Vec<i64> = sqlite::query_one(&db, "SELECT 1, 2, 3", ()).unwrap();
    assert_eq!(cols, [1, 2, 3]);
    let opt: Option<Vec<u8>> = sqlite::query_one(&db, "SELECT NULL", ()).unwrap();
    assert_eq!(opt, None);
}

#[test]
fn unit_and_ignored_columns() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Skip {
        a: i64,
        #[serde(skip)]
        b: i64,
    }
    let db = sqlite::open(":memory:").unwrap();
    let s: Skip = sqlite::query_one(&db, "SELECT 1 AS a, 2 AS b", ()).unwrap();
    assert_eq!(s, Skip { a: 1, b: 0 });
    sqlite::query_one::<(), _>(&db, "SELECT NULL", ()).unwrap();
}
