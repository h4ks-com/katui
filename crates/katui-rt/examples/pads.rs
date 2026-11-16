//! Print every game controller change until interrupted:
//! `cargo run --example pads --features gamepad`.

#![allow(clippy::print_stdout)]

use katui_rt::gamepad::watch;

fn main() {
    let watching = watch(|pads| {
        for pad in pads {
            let pressed: Vec<usize> = (0..pad.buttons.len())
                .filter(|index| pad.buttons.get(*index).is_some_and(|value| *value > 0.5))
                .collect();
            println!(
                "{} {}: buttons {pressed:?} axes {:.2?}",
                pad.index, pad.name, pad.axes
            );
        }
        true
    });
    if let Err(error) = watching {
        println!("{error}");
        return;
    }
    println!("watching game controllers; press buttons, ctrl-c to stop");
    loop {
        std::thread::park();
    }
}
