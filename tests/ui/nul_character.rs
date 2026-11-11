use smop::cmd;

fn main() {
    let _ = cmd!("printf a\0b");
}
