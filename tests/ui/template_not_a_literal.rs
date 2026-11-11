use smop::cmd;

const TEMPLATE: &str = "ls";

fn main() {
    let _ = cmd!(TEMPLATE);
    let _ = cmd!();
    let _ = cmd!("ls {dir}", dir);
    let _ = cmd!("ls" "-la");
}
