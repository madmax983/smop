use smop::cmd;

fn main() {
    let x = "x";
    let _ = cmd!("echo {}");
    let _ = cmd!("echo {0}");
    let _ = cmd!("echo {x:?}");
    let _ = cmd!("echo {a.b}");
    let _ = cmd!("echo { x }");
    let _ = cmd!("echo {x..}");
    let _ = cmd!("echo {r#type}");
    let _ = cmd!("echo {caf\u{e9}}");
    let _ = cmd!("echo {_}");
}
