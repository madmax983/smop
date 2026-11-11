use smop::cmd;

struct Version(u32);

fn main() {
    let version = Version(1);
    let _ = cmd!("echo {version}");

    let ratio = 0.5_f64;
    let _ = cmd!("echo {ratio}");

    let maybe: Option<&str> = None;
    let _ = cmd!("echo --x={maybe}");

    let list = vec!["a"];
    let _ = cmd!("echo {list}");

    let _ = cmd!("echo {flag}", flag = true);
    let _ = version.0;
}
