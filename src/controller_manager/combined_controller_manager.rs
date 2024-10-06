use std::{cell::RefCell, collections::HashMap, rc::Rc};

use super::{
    controller::Controller,
    virtual_controller::{KeyMap, VirtualController},
    ControllerManager, ControllerMessage,
};
use crate::{key_allocator::KeyAllocator, poll_manager::PollManager};

use anyhow::Result as Anyhow;

const COMBINED_GROUP_CAPACITY: usize = 0x100;

#[derive(Debug)]
pub enum Message {}
pub struct CombinedControllerManager {
    combined_group_token_allocator: KeyAllocator,
    controller_groups: HashMap<usize, usize>,
    groups: HashMap<usize, (usize, Vec<(usize, Rc<RefCell<Controller>>)>)>,
}

impl CombinedControllerManager {
    pub fn new() -> Self {
        Self {
            combined_group_token_allocator: KeyAllocator::new(COMBINED_GROUP_CAPACITY),
            controller_groups: HashMap::new(),
            groups: HashMap::new(),
        }
    }

    /// FIXME: remove subscribtions when fail to add new devices.
    pub fn add_new_devices(
        &mut self,
        controllers: Vec<(usize, Rc<RefCell<Controller>>)>,
        keymap: Box<dyn KeyMap>,
        poll_manager: &mut PollManager<ControllerManager, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        let new_group = self.combined_group_token_allocator.allocate()?;
        for controller_token in controllers.iter().map(|controller| controller.0) {
            self.controller_groups.insert(controller_token, new_group);
        }

        let virtual_controller = VirtualController::new(
            controllers
                .iter()
                .map(|(_, controller)| controller.clone())
                .collect(),
            keymap,
        )?;
        let virtual_controller = Rc::new(RefCell::new(virtual_controller));
        let callback = Box::new({
            let virtual_controller = virtual_controller.clone();
            move |_ctx: &mut ControllerManager| {
                virtual_controller.borrow_mut().relay_output_events()?;
                Ok(ControllerMessage::Relay)
            }
        });
        let callback_key = poll_manager.subscribe(
            &*virtual_controller.borrow(),
            polling::Event::readable(0),
            polling::PollMode::Level,
            callback,
        )?;

        let mut sub_controllers = vec![];
        for (id, (_, controller)) in controllers.iter().enumerate() {
            let sub_controller = controller.clone();
            let virtual_controller = virtual_controller.clone();
            let callback = Box::new(move |_ctx: &mut ControllerManager| {
                virtual_controller.borrow_mut().relay_input_events(id)?;
                Ok(ControllerMessage::Relay)
            });

            let callback_key = poll_manager.subscribe(
                &*controller.borrow(),
                polling::Event::readable(0),
                polling::PollMode::Level,
                callback,
            )?;
            sub_controllers.push((callback_key, sub_controller));
        }

        self.groups
            .insert(new_group, (callback_key, sub_controllers));

        Ok(())
    }

    // FIXME: If we remove a controller combined with others, we should return the other
    // controllers.
    pub fn remove_device(
        &mut self,
        _token: usize,
        _poll_manager: &mut PollManager<ControllerManager, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        todo!()
    }
}
