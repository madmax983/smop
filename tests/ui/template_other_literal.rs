use smop::cmd;

fn main() {
    let _ = cmd!(b"ls");
    let _ = cmd!(br"ls");
    let _ = cmd!(c"ls");
    let _ = cmd!(42);
    let _ = cmd!('l');
    let _ = cmd!(-1);
}
