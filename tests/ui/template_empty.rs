use smop::cmd;

fn main() {
    let _ = cmd!("");
    let _ = cmd!(" \n\t");
    let _ = cmd!("'' --version");
}
