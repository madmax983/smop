#![deny(unused_must_use)]

use smop::cmd;

fn main() {
    cmd!("rm -rf target");
}
