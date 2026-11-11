use smop::cmd;

fn main() {
    let argv = ["ls", "-la"];
    let _ = cmd!("{argv...}");
}
