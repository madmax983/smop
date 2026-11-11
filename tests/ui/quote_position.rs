use smop::cmd;

fn main() {
    let _ = cmd!("echo don't");
    let _ = cmd!("echo a\"b\"");
    let _ = cmd!("echo 'abc");
    let _ = cmd!("echo \"abc");
    let _ = cmd!("echo \"a\"b");
}
