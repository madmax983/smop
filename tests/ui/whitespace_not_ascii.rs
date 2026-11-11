use smop::cmd;

fn main() {
    let _ = cmd!("ls\u{a0}-la");
    let _ = cmd!("ls \u{200b}-la");
    let _ = cmd!("ls\x0B-la");
    let _ = cmd!("ls\u{2003}-la");
    let _ = cmd!("ls\u{2028}-la");
    let _ = cmd!("ls\u{3000}-la");
    let _ = cmd!("\u{feff}ls -la");
    let _ = cmd!("echo 'a'\u{a0}b");
}
