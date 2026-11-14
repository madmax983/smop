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

    // In WAL mode, a reader sees the last commit while a write is open.
    let mode: String = sqlite::query_one(&writer, "PRAGMA journal_mode = WAL", ()).unwrap();
    assert_eq!(mode, "wal");
    sqlite::execute(&writer, "BEGIN IMMEDIATE", ()).unwrap();
    sqlite::execute(&writer, "INSERT INTO t VALUES (100)", ()).unwrap();
    let n: i64 = sqlite::query_one(&reader, "SELECT count(*) FROM t", ()).unwrap();
    assert_eq!(n, 1);
    sqlite::execute(&writer, "COMMIT", ()).unwrap();
    sqlite::execute(&writer, "DELETE FROM t WHERE x = 100", ()).unwrap();

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
    assert_eq!(rows, []);
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
    let msg = err(sqlite::query::<i64>(&db, "SELEC 1", ()));
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
    assert!(
        msg.starts_with("SQL error in \"INSERT INTO person (name) VALUES (?1)\""),
        "{msg}"
    );
    assert!(msg.contains("Wrong number of parameters"), "{msg}");
}

#[test]
fn null_into_a_non_option_names_row_and_column() {
    let db = memory();
    add(&db, "ann", Some(1));
    add(&db, "bob", None);
    let sql = "SELECT name, age FROM person ORDER BY id";
    let msg = err(sqlite::query::<(String, u32)>(&db, sql, ()));
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
    let msg = err(sqlite::query::<Age>(&db, "SELECT age FROM person", ()));
    assert!(msg.contains("row 1"), "{msg}");
    assert!(
        msg.contains("column \"age\": invalid type: NULL, expected u32"),
        "{msg}"
    );
}

#[test]
fn out_of_range_integer_is_an_error() {
    let db = sqlite::open(":memory:").unwrap();
    let msg = err(sqlite::query_one::<u8>(&db, "SELECT 300 AS big", ()));
    assert!(msg.contains("column \"big\""), "{msg}");
    assert!(msg.contains("300"), "{msg}");
    let msg = err(sqlite::query_one::<u64>(&db, "SELECT -1", ()));
    assert!(msg.contains("-1"), "{msg}");
}

#[test]
fn wrong_types_are_errors() {
    let db = sqlite::open(":memory:").unwrap();
    assert!(err(sqlite::query_one::<i64>(&db, "SELECT 'x'", ())).contains("expected i64"));
    assert!(err(sqlite::query_one::<i64>(&db, "SELECT 1.5", ())).contains("expected i64"));
    assert!(err(sqlite::query_one::<bool>(&db, "SELECT 'yes'", ())).contains("expected a boolean"));
    assert!(
        err(sqlite::query_one::<String>(&db, "SELECT x'00'", ())).contains("expected a string")
    );
    assert!(
        err(sqlite::query_one::<Role>(&db, "SELECT 'root'", ())).contains("unknown variant `root`")
    );
}

#[test]
fn missing_column_names_the_field() {
    let db = memory();
    add(&db, "ann", None);
    let msg = err(sqlite::query::<Person>(
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
    let msg = err(sqlite::query_one::<i64>(&db, "SELECT 1, 2", ()));
    assert!(msg.contains("2 columns"), "{msg}");
    let msg = err(sqlite::query_one::<(i64, i64)>(&db, "SELECT 1, 2, 3", ()));
    assert!(msg.contains("3 columns"), "{msg}");
    let msg = err(sqlite::query_one::<(i64, i64, i64)>(&db, "SELECT 1, 2", ()));
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
    let msg = err(sqlite::query_one::<Pair>(
        &db,
        "SELECT 1 AS id, 2 AS id",
        (),
    ));
    assert!(msg.contains("duplicate column \"id\""), "{msg}");
    // A map would keep only the last value.
    let msg = err(sqlite::query_one::<BTreeMap<String, i64>>(
        &db,
        "SELECT 1 AS id, 2 AS id",
        (),
    ));
    assert!(msg.contains("duplicate column \"id\""), "{msg}");
    // A tuple reads by position, so the names are not important.
    let pair: (i64, i64) = sqlite::query_one(&db, "SELECT 1 AS id, 2 AS id", ()).unwrap();
    assert_eq!(pair, (1, 2));
}

#[test]
fn query_one_without_rows_quotes_the_statement() {
    let db = memory();
    let msg = err(sqlite::query_one::<i64>(&db, "SELECT id FROM person", ()));
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
        t in any::<String>(),
        o in proptest::option::of(any::<i64>()),
        // SQLite keeps NaN as NULL.
        f in any::<f64>().prop_filter("NaN becomes NULL", |f| !f.is_nan()),
        b in proptest::collection::vec(any::<u8>(), 0..64),
    ) {
        let db = sqlite::open(":memory:").unwrap();
        sqlite::execute(&db, "CREATE TABLE r (i INTEGER, t TEXT, o INTEGER, f REAL, b BLOB)", ()).unwrap();
        sqlite::execute(&db, "INSERT INTO r VALUES (?1, ?2, ?3, ?4, ?5)", params![i, t, o, f, b]).unwrap();
        let got: Row = sqlite::query_one(&db, "SELECT * FROM r", ()).unwrap();
        let tuple: (i64, String, Option<i64>, f64, Vec<u8>) =
            sqlite::query_one(&db, "SELECT * FROM r", ()).unwrap();
        prop_assert_eq!(&tuple, &(i, t.clone(), o, f, b.clone()));
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
    let msg = err(sqlite::query_one::<String>(&db, "SELECT x'6869' AS b", ()));
    assert!(
        msg.contains("column \"b\": invalid type: byte array"),
        "{msg}"
    );
    let msg = err(sqlite::query_one::<char>(&db, "SELECT x'68'", ()));
    assert!(msg.contains("byte array"), "{msg}");
    let msg = err(sqlite::query_one::<Vec<u8>>(&db, "SELECT 'hi' AS t", ()));
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
    sqlite::query_one::<()>(&db, "SELECT NULL", ()).unwrap();
}

#[test]
fn query_opt_gives_none_without_rows() {
    let db = memory();
    let none: Option<i64> = sqlite::query_opt(&db, "SELECT id FROM person", ()).unwrap();
    assert_eq!(none, None);
    add(&db, "ann", Some(3));
    let found: Option<(String, Option<i64>)> =
        sqlite::query_opt(&db, "SELECT name, age FROM person WHERE name = ?1", ["ann"]).unwrap();
    assert_eq!(found, Some(("ann".into(), Some(3))));
    // A NULL column is a row, not "no row".
    let null: Option<Option<i64>> = sqlite::query_opt(&db, "SELECT NULL", ()).unwrap();
    assert_eq!(null, Some(None));
}

#[test]
fn query_errors_quote_the_statement() {
    let db = memory();
    add(&db, "ann", None);
    // The error comes from a step, not from prepare.
    let sql = "INSERT INTO person (name) VALUES ('ann') RETURNING id";
    let msg = err(sqlite::query::<i64>(&db, sql, ()));
    assert!(msg.starts_with(&format!("SQL error in {sql:?}")), "{msg}");
    assert!(msg.contains("UNIQUE"), "{msg}");

    let msg = err(sqlite::query::<i64>(&db, "SELECT ?1", ()));
    assert!(msg.starts_with("SQL error in \"SELECT ?1\""), "{msg}");

    let msg = err(sqlite::query::<i64>(&db, "SELECT 1; SELECT 2", ()));
    assert!(
        msg.starts_with("SQL error in \"SELECT 1; SELECT 2\""),
        "{msg}"
    );

    let msg = err(sqlite::execute(&db, "SELECT 1", ()));
    assert!(msg.starts_with("SQL error in \"SELECT 1\""), "{msg}");
    assert!(msg.contains("query"), "{msg}");
}

#[test]
fn text_that_is_not_utf8_names_the_column() {
    let db = sqlite::open(":memory:").unwrap();
    let msg = err(sqlite::query_one::<String>(
        &db,
        "SELECT CAST(x'ff' AS TEXT) AS t",
        (),
    ));
    assert!(msg.starts_with("Failed to read row 1 of"), "{msg}");
    assert!(msg.contains("column \"t\""), "{msg}");
}

#[test]
fn null_into_fields_that_are_not_options() {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct S {
        s: String,
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct B {
        b: bool,
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct E {
        r: Role,
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct V {
        v: Vec<u8>,
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct A {
        a: [u8; 2],
    }
    let db = sqlite::open(":memory:").unwrap();
    for msg in [
        err(sqlite::query_one::<S>(&db, "SELECT NULL AS s", ())),
        err(sqlite::query_one::<B>(&db, "SELECT NULL AS b", ())),
        err(sqlite::query_one::<E>(&db, "SELECT NULL AS r", ())),
        err(sqlite::query_one::<V>(&db, "SELECT NULL AS v", ())),
        err(sqlite::query_one::<A>(&db, "SELECT NULL AS a", ())),
    ] {
        assert!(msg.contains("invalid type: NULL"), "{msg}");
    }
    let msg = err(sqlite::query_one::<E>(&db, "SELECT 1 AS r", ()));
    assert!(
        msg.contains("column \"r\": invalid type: integer `1`"),
        "{msg}"
    );
    let msg = err(sqlite::query_one::<V>(&db, "SELECT 'x' AS v", ()));
    assert!(msg.contains("column \"v\""), "{msg}");
}

#[test]
fn row_shapes() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Pair(i64, i64);
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Unit;
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Wrap(Person2);
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Person2 {
        name: String,
    }
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Hash {
        h: [u8; 2],
    }
    let db = sqlite::open(":memory:").unwrap();
    assert_eq!(
        sqlite::query_one::<Pair>(&db, "SELECT 1, 2", ()).unwrap(),
        Pair(1, 2)
    );
    assert!(err(sqlite::query_one::<Pair>(&db, "SELECT 1", ())).contains("got 1 column"));
    assert!(err(sqlite::query_one::<(i64,)>(&db, "SELECT 1, 2", ())).contains("expected 1 column"));
    assert_eq!(
        sqlite::query_one::<Unit>(&db, "SELECT NULL", ()).unwrap(),
        Unit
    );
    assert_eq!(
        sqlite::query_one::<Wrap>(&db, "SELECT 'ann' AS name, 1 AS x", ()).unwrap(),
        Wrap(Person2 { name: "ann".into() })
    );
    // A one-column BLOB row into an array, and an array field.
    let h: [u8; 4] = sqlite::query_one(&db, "SELECT x'01020304'", ()).unwrap();
    assert_eq!(h, [1, 2, 3, 4]);
    let msg = err(sqlite::query_one::<[u8; 4]>(&db, "SELECT x'0102'", ()));
    assert!(msg.contains("invalid length 2"), "{msg}");
    assert_eq!(
        sqlite::query_one::<Hash>(&db, "SELECT x'0a0b' AS h", ()).unwrap(),
        Hash { h: [10, 11] }
    );
    let ignored: serde::de::IgnoredAny = sqlite::query_one(&db, "SELECT 1, 2", ()).unwrap();
    let _ = ignored;
}

#[test]
fn flatten_and_untagged_fields() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Inner {
        n: u32,
        b: Vec<u8>,
    }
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Outer {
        id: i64,
        #[serde(flatten)]
        inner: Inner,
    }
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    #[serde(untagged)]
    enum Either {
        N(i64),
        S(String),
    }
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Mixed {
        a: Either,
        b: Either,
    }
    let db = sqlite::open(":memory:").unwrap();
    let o: Outer = sqlite::query_one(&db, "SELECT 1 AS id, 3 AS n, x'00ff' AS b", ()).unwrap();
    assert_eq!(
        o,
        Outer {
            id: 1,
            inner: Inner {
                n: 3,
                b: vec![0, 255]
            }
        }
    );
    let m: Mixed = sqlite::query_one(&db, "SELECT 5 AS a, 'x' AS b", ()).unwrap();
    assert_eq!(
        m,
        Mixed {
            a: Either::N(5),
            b: Either::S("x".into())
        }
    );
}

#[cfg(feature = "json")]
#[test]
fn a_blob_in_a_json_value_is_an_array_of_bytes() {
    let db = sqlite::open(":memory:").unwrap();
    let v: smop::serde_json::Value = sqlite::query_one(&db, "SELECT x'0102' AS b", ()).unwrap();
    assert_eq!(v, smop::serde_json::json!({"b": [1, 2]}));
}

#[test]
fn a_savepoint_derefs_to_a_connection() {
    let mut db = memory();
    let mut tx = db.transaction().unwrap();
    {
        let sp = tx.savepoint().unwrap();
        sqlite::execute(&sp, "INSERT INTO person (name) VALUES ('s')", ()).unwrap();
        sp.commit().unwrap();
    }
    let n: i64 = sqlite::query_one(&tx, "SELECT count(*) FROM person", ()).unwrap();
    assert_eq!(n, 1);
    tx.commit().unwrap();
}

#[test]
fn an_empty_path_is_an_error() {
    let msg = err(sqlite::open(""));
    assert_eq!(msg, "Failed to open SQLite database: the path is empty");
    let msg = err(sqlite::open_read_only(""));
    assert_eq!(
        msg,
        "Failed to open SQLite database for reads: the path is empty"
    );
    let msg = err(sqlite::open_read_only(":memory:"));
    assert!(msg.contains(":memory: is not a file"), "{msg}");
}

#[test]
fn empty_sql_is_an_error() {
    let db = sqlite::open(":memory:").unwrap();
    let want = "SQL error in \"\": the statement is empty";
    assert_eq!(err(sqlite::execute(&db, "", ())), want);
    assert_eq!(err(sqlite::query::<i64>(&db, "  \n", ())), want);
    assert_eq!(err(sqlite::query_one::<i64>(&db, " ", ())), want);
}

/// A path that starts with `file:` is a file name, not a URI.
#[cfg(unix)]
#[test]
fn a_file_uri_is_a_plain_path() {
    /// Removes the directory at drop, also after a failed assert.
    struct Remove(String);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    // Relative to the package root, the working directory of a test.
    let dir = Remove(format!("file:smop-sqlite-test-{}", std::process::id()));
    std::fs::create_dir(&dir.0).unwrap();
    let name = format!("{}/q.db?mode=memory", dir.0);
    for _ in 0..2 {
        let db = sqlite::open(&name).unwrap();
        sqlite::execute(&db, "CREATE TABLE IF NOT EXISTS t (x INTEGER)", ()).unwrap();
        sqlite::execute(&db, "INSERT INTO t VALUES (1)", ()).unwrap();
    }
    assert!(Path::new(&name).is_file(), "no file {name}");
    let db = sqlite::open_read_only(&name).unwrap();
    let n: i64 = sqlite::query_one(&db, "SELECT count(*) FROM t", ()).unwrap();
    assert_eq!(n, 2, "the data is in the file, not in memory");
}

#[test]
fn open_does_not_wait_for_a_lock() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("busy.db");
    let writer = sqlite::open(&file).unwrap();
    sqlite::execute(&writer, "CREATE TABLE t (x INTEGER)", ()).unwrap();
    sqlite::execute(&writer, "BEGIN EXCLUSIVE", ()).unwrap();
    sqlite::execute(&writer, "INSERT INTO t VALUES (1)", ()).unwrap();

    let start = std::time::Instant::now();
    let reader = sqlite::open_read_only(&file).unwrap();
    let other = sqlite::open(&file).unwrap();
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "open waited {:?}",
        start.elapsed()
    );

    sqlite::execute(&writer, "COMMIT", ()).unwrap();
    let n: i64 = sqlite::query_one(&reader, "SELECT count(*) FROM t", ()).unwrap();
    assert_eq!(n, 1);
    let n: i64 = sqlite::query_one(&other, "SELECT count(*) FROM t", ()).unwrap();
    assert_eq!(n, 1);
}
