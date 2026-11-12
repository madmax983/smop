//! Profiling harness: `valgrind --tool=callgrind target/release/examples/bolt_workload`.
use smop::prelude::*;
fn main() -> Result<()> {
    let t = fs::temp_dir()?;
    let root = t.path().join("tree");
    for d in 0..50 {
        for f in 0..40 {
            let dir = root.join(format!("d{d}/sub{}", f % 5));
            fs::create_dir_all(&dir)?;
            fs::write_string(dir.join(format!("f{f}.txt")), "line one\nline two\n".repeat(200))?;
        }
    }
    let files = fs::walk(&root)?;
    let mut n = 0;
    for f in &files { n += fs::read_lines(f)?.len(); }
    fs::copy_dir(&root, t.path().join("copy"))?;
    let h = files.iter().map(|f| hash::sha256_file(f)).collect::<Result<Vec<_>>>()?;
    println!("{} files, {n} lines, {} hashes", files.len(), h.len());
    Ok(())
}
