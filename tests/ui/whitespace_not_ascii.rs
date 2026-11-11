use smop::cmd;

fn main() {
    let _ = cmd!("ls\u{a0}-la");
    let _ = cmd!("ls \u{200b}-la");
}
