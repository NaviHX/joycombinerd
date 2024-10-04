use std::{cell::RefCell, rc::Rc};

use super::{controller::Controller, virtual_controller::KeyMap, ControllerManager, ControllerMessage};
use crate::poll_manager::PollManager;

use anyhow::Result as Anyhow;

#[derive(Debug)]
pub enum Message {}
pub struct CombinedControllerManager;

impl CombinedControllerManager {
    pub fn new() -> Self {
        todo!()
    }

    pub fn add_new_devices(
        &self,
        _controllers: Vec<(usize, Rc<RefCell<Controller>>, Box<dyn KeyMap>)>,
        _poll_manager: &mut PollManager<ControllerManager, Anyhow<ControllerMessage>>,
    ) -> Anyhow<()> {
        todo!()
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
