use smop::cmd;

fn main() {
    let version = "1.2.0";
    let _ = cmd!("git tag {verison}");
    let _ = version;
}
