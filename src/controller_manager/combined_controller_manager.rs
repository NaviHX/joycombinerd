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
    groups: HashMap<
        usize,
        (
            usize,
            Rc<RefCell<VirtualController>>,
            Vec<(usize, usize, Rc<RefCell<Controller>>)>,
        ),
    >,
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
        for (id, (token, controller)) in controllers.iter().enumerate() {
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
            sub_controllers.push((callback_key, *token, sub_controller));
        }

        self.groups.insert(
            new_group,
            (callback_key, virtual_controller, sub_controllers),
        );

        Ok(())
    }

    pub fn remove_device(
        &mut self,
        remove_token: usize,
        poll_manager: &mut PollManager<ControllerManager, Anyhow<ControllerMessage>>,
    ) -> Anyhow<Vec<(usize, Rc<RefCell<Controller>>)>> {
        let group = self.controller_groups.get(&remove_token).ok_or_else(|| {
            anyhow::anyhow!("Failed to find combined group for controller token {remove_token}")
        })?;
        let (callback_key, virtual_controller, sub_controllers) = self.groups.remove(&group).ok_or_else(|| {
            anyhow::anyhow!("Failed to get combined group info for group {group}")
        })?;
        self.combined_group_token_allocator.release(*group);

        // Remove virtual controller subscribtion.
        poll_manager.remove(callback_key, &*virtual_controller.borrow())?;

        // Remove each controllers subscribtion and collect controllers except the one to be
        // removed.
        let mut collected = vec![];
        for (callback_key, token, controller) in sub_controllers {
            poll_manager.remove(callback_key, &*controller.borrow())?;
            self.controller_groups.remove(&token);

            if token != remove_token {
                collected.push((token, controller));
            }
        }

        Ok(collected)
    }
}
