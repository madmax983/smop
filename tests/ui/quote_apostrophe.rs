use smop::cmd;

fn main() {
    let _ = cmd!("echo 'don't'");
    let _ = cmd!("git commit -m 'Don't break' --author 'O'Brien'");
}
