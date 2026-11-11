use smop::cmd;

fn main() {
    let _ = cmd!("echo {x");
    let _ = cmd!("echo x}");
}
