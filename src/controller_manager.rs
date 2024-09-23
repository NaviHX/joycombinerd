use combined_controller_manager::{CombinedControllerManager, Message as CombinedMessage};
use lone_controller_manager::{LoneControllerManager, Message as LoneMessage};
use waiting_controller_manager::{Message as WaitingMessage, WaitingControllerManager};

use crate::poll_manager::PollManager;

use anyhow::Result as Anyhow;

mod combined_controller_manager;
mod lone_controller_manager;
mod waiting_controller_manager;
mod udev_detector;

pub enum ControllerMessage {
    Waiting(WaitingMessage),
    Lone(LoneMessage),
    Combined(CombinedMessage),
}

impl ControllerMessage {
    fn process(
        self,
        key: usize,
        controller_manager: &mut ControllerManager,
        poll_manager: &mut PollManager<ControllerManager, Self>,
    ) -> Anyhow<()> {
        match self {
            ControllerMessage::Waiting(_) => todo!(),
            ControllerMessage::Lone(_) => todo!(),
            ControllerMessage::Combined(_) => todo!(),
        }
    }
}

pub struct ControllerManager {
    waiting_controller_manager: WaitingControllerManager,
    lone_controller_manager: LoneControllerManager,
    combined_controller_manager: CombinedControllerManager,
}

impl ControllerManager {
    pub fn poll(&mut self, poll_manager: &mut PollManager<Self, ControllerMessage>) -> Anyhow<()> {
        let messages = poll_manager.poll(self)?;

        for message in messages {
            match message {
                Ok(message) => {
                    let (key, message) = message;
                    if let Err(e) = message.process(key, self, poll_manager) {
                        eprintln!("{e}");
                    }
                }
                Err(e) => eprintln!("{e}"),
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
}
