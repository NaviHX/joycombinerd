use combined_controller_manager::{CombinedControllerManager, Message as CombinedMessage};
use lone_controller_manager::{LoneControllerManager, Message as LoneMessage};
use waiting_controller_manager::{Message as WaitingMessage, WaitingControllerManager};

use crate::{
    poll_manager::{self, PollManager},
    udev_detector::JoyconUdevDetector,
    UDEV_KEY,
};

use anyhow::{Context, Result as Anyhow};

mod combined_controller_manager;
mod controller;
mod virtual_controller;
mod lone_controller_manager;
mod waiting_controller_manager;

#[allow(unused)]
#[derive(Debug)]
pub enum ControllerMessage {
    Waiting(WaitingMessage),
    Lone(LoneMessage),
    Combined(CombinedMessage),

    UdevEvent(udev::Event),
    DeviceScan(udev::Device),
}

impl ControllerMessage {
    pub fn process(
        self,
        _key: usize,
        _controller_manager: &mut ControllerManager,
        _poll_manager: &mut PollManager<ControllerManager, Anyhow<Self>>,
    ) -> Anyhow<()> {
        eprintln!("{self:?}");

        // match self {
        //     ControllerMessage::Waiting(_) => todo!(),
        //     ControllerMessage::Lone(_) => todo!(),
        //     ControllerMessage::Combined(_) => todo!(),
        //     ControllerMessage::UdevEvent(_) => todo!(),
        // }

        Ok(())
    }
}

#[allow(unused)]
pub struct ControllerManager {
    waiting_controller_manager: WaitingControllerManager,
    lone_controller_manager: LoneControllerManager,
    combined_controller_manager: CombinedControllerManager,
}

impl ControllerManager {
    pub fn poll(
        &mut self,
        poll_manager: &mut PollManager<Self, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        let messages = poll_manager.poll(self)?;

        for message in messages {
            if let Err(e) = message.and_then(|msg| {
                let (key, msg) = msg;
                msg.and_then(|msg| msg.process(key, self, poll_manager))
            }) {
                eprintln!("{e}");
            }
        }

        Ok(())
    }

    pub fn new() -> Self {
        Self {
            waiting_controller_manager: WaitingControllerManager::new(),
            lone_controller_manager: LoneControllerManager::new(),
            combined_controller_manager: CombinedControllerManager::new(),
        }
    }

    pub fn init(
        &mut self,
        poll_manager: &mut PollManager<Self, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        let devices =
            JoyconUdevDetector::enumerate().with_context(|| "Failed to scan the udev devices")?;
        for msg in devices.into_iter().map(ControllerMessage::DeviceScan) {
            msg.process(UDEV_KEY, self, poll_manager)?
        }

        Ok(())
    }
}
