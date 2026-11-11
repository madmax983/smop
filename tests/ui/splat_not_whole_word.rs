use smop::cmd;

fn main() {
    let xs = ["a", "b"];
    let _ = cmd!("ls --x={xs...}");
    let _ = cmd!("ls '{xs...}'");
    let _ = cmd!("ls {xs...}{xs...}");
}
