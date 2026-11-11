use smop::cmd;

fn main() {
    let _ = cmd!("echo {fn}");
    let _ = cmd!("echo {type...}");
    let _ = cmd!("echo {do}");
    let _ = cmd!("echo {yield}");
}
