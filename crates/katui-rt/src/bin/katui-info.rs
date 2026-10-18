// printing is this binary's whole job
#![allow(clippy::print_stdout, clippy::print_stderr)]

#[cfg(unix)]
fn main() {
    let env = katui_proto::probe::Env::capture();
    let caps = match katui_rt::tty::StdTransport::open(std::time::Duration::from_millis(300)) {
        Ok(mut transport) => katui_proto::probe::probe(&mut transport, &env),
        Err(error) => {
            eprintln!("katui-info: no controlling terminal to probe ({error}), sniffing only");
            katui_proto::probe::sniff(&env)
        }
    };
    print!("{}", katui_rt::info::card(&caps));
}

#[cfg(not(unix))]
fn main() {
    println!("katui-info needs a controlling terminal");
}
