use controller_manager::{ControllerManager, ControllerMessage};
use poll_manager::PollManager;

mod controller_manager;
mod poll_manager;

fn main() {
    println!("Joycombindered starts!");

    let mut controller_manager: ControllerManager = todo!("Create a controller manger");
    let mut poll_manager: PollManager<ControllerManager, ControllerMessage> = todo!("Create a poll manager");

    loop {
        if let Err(e) = controller_manager.poll(&mut poll_manager) {
            eprintln!("{e}");
        }
    }
}
