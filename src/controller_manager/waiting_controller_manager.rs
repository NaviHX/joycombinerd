use std::{cell::RefCell, rc::Rc};

use anyhow::Result as Anyhow;

use crate::poll_manager::PollManager;

use super::{controller::Controller, ControllerManager, ControllerMessage};

pub struct WaitingControllerManager;

impl WaitingControllerManager {
    pub fn new() -> Self {
        todo!()
    }

    pub fn add_new_device(
        &self,
        _token: usize,
        _controller: Rc<RefCell<Controller>>,
        _poll_manager: &mut PollManager<ControllerManager, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        todo!()
    }

    pub fn remove_device(
        &self,
        _token: usize,
        _poll_manager: &mut PollManager<ControllerManager, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        todo!()
    }

    pub fn get_controller(&self, token: usize) -> Anyhow<Rc<RefCell<Controller>>> {
        todo!()
    }
}
