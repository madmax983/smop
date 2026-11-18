//! Profiling harness for the yaml module:
//! `valgrind --tool=callgrind target/release/examples/bolt_yaml`.
use smop::prelude::*;
use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Serialize, Deserialize)]
#[serde(crate = "smop::serde")]
struct Job {
    name: String,
    image: String,
    ports: Vec<String>,
    env: BTreeMap<String, String>,
    steps: Vec<Step>,
    retries: u32,
    enabled: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "smop::serde")]
struct Step {
    run: String,
    timeout: f64,
}

#[derive(Deserialize)]
#[serde(crate = "smop::serde")]
struct Doc {
    jobs: BTreeMap<String, Job>,
}

fn document(jobs: usize) -> String {
    let mut s = String::from("defaults: &d\n  retries: 3\n  enabled: true\njobs:\n");
    for i in 0..jobs {
        let _ = write!(
            s,
            "  job{i}:\n    <<: *d\n    name: \"Job number {i}\"\n    image: registry.example.com/team/app:{i}.1\n"
        );
        s += "    ports: [\"80:80\", \"443:443\"]\n    env:\n      RUST_LOG: debug\n      PATH_X: /usr/local/bin:/usr/bin\n    steps:\n";
        for k in 0..6 {
            let _ = write!(
                s,
                "      - run: cargo build --release -p crate{k}\n        timeout: {k}.5\n"
            );
        }
    }
    s
}

fn main() -> Result<()> {
    let text = document(300);
    let v: yaml::Value = yaml::from_str(&text)?;
    let jobs = yaml::from_str::<Doc>(&text)?.jobs;
    let out = yaml::to_string(&jobs)?;
    println!(
        "{} bytes in, {} jobs, {} bytes out, {:?}",
        text.len(),
        jobs.len(),
        out.len(),
        v["jobs"]["job0"]["name"].as_str()
    );
    Ok(())
}
