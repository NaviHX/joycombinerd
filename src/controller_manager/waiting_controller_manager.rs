use std::{cell::RefCell, collections::HashMap, rc::Rc};

use anyhow::Result as Anyhow;
use polling::Event;

use crate::poll_manager::PollManager;

use super::{controller::Controller, ControllerManager, ControllerMessage};

pub struct WaitingControllerManager {
    controllers: HashMap<usize, (usize, Rc<RefCell<Controller>>)>,
}

impl WaitingControllerManager {
    pub fn new() -> Self {
        Self {
            controllers: HashMap::new(),
        }
    }

    pub fn add_new_device(
        &mut self,
        token: usize,
        controller: Rc<RefCell<Controller>>,
        poll_manager: &mut PollManager<ControllerManager, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        let callback = Box::new({
            let controller = controller.clone();
            move |_ctx: &mut ControllerManager| {
                controller
                    .borrow_mut()
                    .handle_pairing_events()
                    .map(|state| ControllerMessage::StateUpdate(token, state))
            }
        });

        let callback_key = poll_manager.subscribe(
            &*controller.borrow(),
            Event::readable(0),
            polling::PollMode::Level,
            callback,
        )?;
        self.controllers.insert(token, (callback_key, controller));

        Ok(())
    }

    pub fn remove_device(
        &mut self,
        token: usize,
        poll_manager: &mut PollManager<ControllerManager, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        let (callback_key, controller) = self
            .controllers
            .remove(&token)
            .ok_or_else(|| anyhow::anyhow!("No controller for token {token} to remove"))?;
        poll_manager.remove(callback_key, &*controller.borrow())?;
        Ok(())
    }

    pub fn get_controller(&self, token: usize) -> Anyhow<Rc<RefCell<Controller>>> {
        self.controllers
            .get(&token)
            .ok_or_else(|| anyhow::anyhow!("No controller for token {token}"))
            .map(|c| c.1.clone())
    }
}
