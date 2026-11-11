use smop::cmd;

fn main() {
    let text = String::from("a b");
    let _ = cmd!("ls {text...}");

    let numbers = vec![1, 2];
    let _ = cmd!("ls {numbers...}");

    let one_shot = ["a"].into_iter().map(str::to_uppercase);
    let _ = cmd!("ls {one_shot...}");
}
