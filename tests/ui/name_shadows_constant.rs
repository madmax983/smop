use smop::cmd;

#[allow(non_upper_case_globals)]
const dir: &str = "/";

fn main() {
    let _ = cmd!("ls {dir}", dir = "src");
}
