// printing is this binary's whole job
#![allow(clippy::print_stdout, clippy::print_stderr)]

fn main() {
    let caps = katui_rt::tty::probe_default();
    print!("{}", katui_rt::info::card(&caps));
}
