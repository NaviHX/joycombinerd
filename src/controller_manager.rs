use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

use combined_controller_manager::CombinedControllerManager;
use controller::{Controller, PairingState};
use virtual_controller::key_map::{self, CombinedControllerKeyMap};
use waiting_controller_manager::WaitingControllerManager;

use crate::{
    key_allocator::KeyAllocator, poll_manager::PollManager, udev_detector::JoyconUdevDetector,
    UDEV_KEY,
};

use anyhow::{anyhow, Context, Result as Anyhow};

mod combined_controller_manager;
mod controller;
mod lone_controller_manager;
mod virtual_controller;
mod waiting_controller_manager;

const CONTROLLER_TOKEN_CAPACITY: usize = 0x100;

#[allow(unused)]
#[derive(Debug)]
pub enum ControllerMessage {
    StateUpdate(usize, PairingState),

    UdevEvent(udev::Event),
    DeviceScan(udev::Device),

    Relay,
}

#[allow(unused)]
pub struct ControllerManager {
    waiting_controller_manager: WaitingControllerManager,
    combined_controller_manager: CombinedControllerManager,

    controller_token_allocator: KeyAllocator,
    controller_token_map: HashMap<PathBuf, usize>,

    left: Option<usize>,
    right: Option<usize>,
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
                msg.and_then(|msg| self.process(key, poll_manager, msg))
            }) {
                eprintln!("{e}");
            }
        }

        Ok(())
    }

    pub fn new() -> Self {
        Self {
            waiting_controller_manager: WaitingControllerManager::new(),
            combined_controller_manager: CombinedControllerManager::new(),
            controller_token_allocator: KeyAllocator::new(CONTROLLER_TOKEN_CAPACITY),
            controller_token_map: HashMap::new(),
            left: None,
            right: None,
        }
    }

    pub fn init(
        &mut self,
        poll_manager: &mut PollManager<Self, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        let devices =
            JoyconUdevDetector::enumerate().with_context(|| "Failed to scan the udev devices")?;
        for msg in devices.into_iter().map(ControllerMessage::DeviceScan) {
            self.process(UDEV_KEY, poll_manager, msg)?
        }

        Ok(())
    }

    pub fn process(
        &mut self,
        _callback_key: usize,
        poll_manager: &mut PollManager<Self, Anyhow<ControllerMessage>>,
        message: ControllerMessage,
    ) -> Anyhow<()> {
        eprintln!("{message:?}");

        match message {
            ControllerMessage::StateUpdate(token, state) => {
                self.update_pairing_state(token, state, poll_manager)?;
            }
            ControllerMessage::UdevEvent(event) => match event.event_type() {
                udev::EventType::Add => {
                    self.add_new_device(event.device(), poll_manager)?;
                }
                udev::EventType::Remove => {
                    self.remove_device(event.device(), poll_manager)?;
                }
                _ => Err(anyhow!("Unhandled udev event: {event:?}"))?,
            },
            ControllerMessage::DeviceScan(device) => {
                self.add_new_device(device, poll_manager)?;
            }

            ControllerMessage::Relay => {
                // Do nothing.
            }
        }

        Ok(())
    }

    /// Add a new controller to the controller manager and generate a token for it. The new controller will be added to the
    /// waiting controller manager.
    fn add_new_device(
        &mut self,
        device: udev::Device,
        poll_manager: &mut PollManager<Self, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        let new_key = self.controller_token_allocator.allocate()?;
        let devname = device
            .devnode()
            .ok_or_else(|| anyhow::anyhow!("Cannot get devnode of {:?}", device.devpath()))?;
        self.controller_token_map
            .insert(devname.to_path_buf(), new_key);
        let controller = Rc::new(RefCell::new(Controller::new(device)?));
        self.waiting_controller_manager
            .add_new_device(new_key, controller, poll_manager)?;

        Ok(())
    }

    /// Remove a controller.
    fn remove_device(
        &mut self,
        device: udev::Device,
        poll_manager: &mut PollManager<Self, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        let devpath = device
            .devnode()
            .ok_or_else(|| anyhow::anyhow!("Cannot get devnode of {:?}", device.devpath()))?;
        let &token = self
            .controller_token_map
            .get(devpath)
            .ok_or_else(|| anyhow::anyhow!("Cannot get token of {:?}", device.devpath()))?;

        let collected = if self.waiting_controller_manager.remove_device(token, poll_manager)?.is_some() {
            Some(vec![])
        } else {
            self.combined_controller_manager.remove_device(token, poll_manager)?
        };

        if let Some(controllers) = collected {
            for (token, controller) in controllers {
                self.waiting_controller_manager.add_new_device(token, controller, poll_manager)?
            }
        } else {
            Err(anyhow::anyhow!("Token {token} cannot be found in neither waiting controllers nor combined controllers"))?
        }

        Ok(())
    }

    /// Update the controller manager when receiving paring states.
    fn update_pairing_state(
        &mut self,
        controller_token: usize,
        pairing_state: PairingState,
        poll_manager: &mut PollManager<Self, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        match pairing_state {
            PairingState::Pairing => {
                // Do nothing.
            }
            PairingState::Waiting(model) => {
                // Store the controller token and corresponding model. Combine the controllers
                // when both left and right controllers are entering waiting state.
                match model {
                    controller::Model::LeftJoycon => self.left = Some(controller_token),
                    controller::Model::RightJoycon => self.right = Some(controller_token),
                }

                if self.left.is_some() && self.right.is_some() {
                    let (left_token, right_token) = (
                        self.left
                            .take()
                            .ok_or_else(|| anyhow::anyhow!("Left token is None"))?,
                        self.right
                            .take()
                            .ok_or_else(|| anyhow::anyhow!("Right token is None"))?,
                    );

                    let (left_controller, right_controller) = (
                        self.waiting_controller_manager.get_controller(left_token)?,
                        self.waiting_controller_manager
                            .get_controller(right_token)?,
                    );

                    self.waiting_controller_manager
                        .remove_device(left_token, poll_manager)?;
                    self.waiting_controller_manager
                        .remove_device(right_token, poll_manager)?;

                    self.combined_controller_manager.add_new_devices(
                        vec![
                            (left_token, left_controller),
                            (right_token, right_controller),
                        ],
                        Box::new(CombinedControllerKeyMap::new()),
                        poll_manager,
                    )?;
                }
            }

            // Push the controller into combined controller manager and configure it with
            // corresponding key map.
            PairingState::Lone => {
                let controller = self
                    .waiting_controller_manager
                    .get_controller(controller_token)?;
                self.waiting_controller_manager
                    .remove_device(controller_token, poll_manager)?;
                self.combined_controller_manager.add_new_devices(
                    vec![(controller_token, controller)],
                    Box::new(key_map::LoneConstrollerKeyMap::new()),
                    poll_manager,
                )?;
            }
            PairingState::Horizontal => {
                let controller = self
                    .waiting_controller_manager
                    .get_controller(controller_token)?;
                self.waiting_controller_manager
                    .remove_device(controller_token, poll_manager)?;
                let key_map = match controller.borrow().get_model() {
                    controller::Model::LeftJoycon => key_map::HorizontalLeftControllerKeyMap::new(),
                    controller::Model::RightJoycon => {
                        key_map::HorizontalRightControllerKeyMap::new()
                    }
                };
                self.combined_controller_manager.add_new_devices(
                    vec![(controller_token, controller)],
                    Box::new(key_map),
                    poll_manager,
                )?;
            }
        }

        Ok(())
    }
}
