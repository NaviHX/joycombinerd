use anyhow::Result as Anyhow;
use controller_manager::ControllerManager;
use poll_manager::PollManager;
use std::os::fd::AsRawFd;
use udev_detector::JoyconUdevDetector;

mod controller_manager;
mod key_allocator;
mod poll_manager;
mod udev_detector;

use poll_manager::KEY_CAPACITY;
const UDEV_KEY: usize = KEY_CAPACITY - 1;

fn main() -> Anyhow<()> {
    println!("Joycombindered starts!");

    let mut controller_manager = ControllerManager::new();
    let mut poll_manager = PollManager::new()?;
    controller_manager.init(&mut poll_manager)?;

    // Create the first ever udev monitor add register the callback.
    let udev_monitor = JoyconUdevDetector::monitor()?;
    let epfd = udev_monitor.as_raw_fd();
    let callback = JoyconUdevDetector::callback(udev_monitor);
    poll_manager.subscribe_with_key(
        UDEV_KEY,
        epfd,
        polling::Event::readable(0),
        polling::PollMode::Level,
        Box::new(callback),
    )?;

    loop {
        if let Err(e) = controller_manager.poll(&mut poll_manager) {
            eprintln!("{e}");
        }
    }
}
