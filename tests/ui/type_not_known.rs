use smop::cmd;

fn bind(_port: u16) {}

fn changed() -> Vec<String> {
    let files = "a b".split(' ').map(str::to_owned).collect();
    let _ = cmd!("rustfmt {files...}");
    files
}

fn listed() {
    let _ = cmd!("nc -l {p}", p = "80".parse().unwrap());
}

fn main() {
    let port = "80".parse().unwrap();
    let _ = cmd!("nc -l {port}");
    bind(port);

    let _ = changed();
    listed();
}
