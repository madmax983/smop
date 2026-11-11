use smop::cmd;

fn main() {
    let f = "done";
    let _ = cmd!("rm -f {f} && echo done");
    let _ = cmd!("make 2>/dev/null");
    let _ = cmd!("ls | wc -l");
    let _ = cmd!("sleep 1 &");
    let _ = cmd!("find . -exec rm {{}} ;");
    let _ = cmd!("sort <input");
    let _ = cmd!("echo `id`");
    let _ = cmd!("echo $HOME");
    let _ = cmd!("echo $(id)");
    let _ = cmd!("echo ${{HOME}}");
    let _ = cmd!("ls ~/src");
    let _ = cmd!("echo #comment");
    let _ = cmd!("echo $1");
    let _ = cmd!("echo $_x");
    let _ = cmd!("echo ${f}");
    let _ = cmd!("curl http://x/?a=1&b=2");
    let _ = cmd!("a|b");
    let _ = cmd!("{f};{f}");
}
