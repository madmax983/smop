use smop::cmd;

fn main() {
    let _ = cmd!("ls {dir}", dir = ".", extra = "x");
}
