use smop::cmd;

fn main() {
    let _ = cmd!("echo {fn}");
    let _ = cmd!("echo {type...}");
}
