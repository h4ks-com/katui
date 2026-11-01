// printing is this binary's whole job
#![allow(clippy::print_stdout, clippy::print_stderr)]

#[cfg(unix)]
fn main() {
    let caps = katui_rt::tty::probe_default();
    print!("{}", katui_rt::info::card(&caps));
}

#[cfg(not(unix))]
fn main() {
    println!("katui-info needs a controlling terminal");
}
