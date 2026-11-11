use smop::cmd;

fn main() {
    let text = String::from("a b");
    let _ = cmd!("ls {text...}");

    let slice: &str = &text;
    let _ = cmd!("ls {slice...}");

    let numbers = vec![1, 2];
    let _ = cmd!("ls {numbers...}");

    let one_shot = ["a"].into_iter().map(str::to_uppercase);
    let _ = cmd!("ls {one_shot...}");

    // An `Err` must not become zero arguments.
    let found: Result<String, std::io::Error> = Ok(String::from("a"));
    let _ = cmd!("ls {found...}");
    let _ = cmd!("ls {f...}", f = &found);
}
