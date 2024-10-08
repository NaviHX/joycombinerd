#![allow(unused)]

use std::os::fd::{AsFd, AsRawFd, BorrowedFd};

use anyhow::Result as Anyhow;
use evdev::{Device, FetchEventsSynced, InputEvent};

// TODO: Support motion device.
pub struct Controller {
    device: Device,
    buttons_state: ButtonsState,
    model: Model,
}

impl Controller {
    pub fn new(device: udev::Device) -> Anyhow<Self> {
        let devname = device
            .devnode()
            .ok_or_else(|| anyhow::anyhow!("Failed to get devnode"))?;
        let _devpath = device.devpath();

        let device = Device::open(devname)?;
        let product_id = device.input_id().product();
        let model = Model::from_product_id(product_id)?;
        let buttons_state = ButtonsState::default();

        Ok(Self {
            device,
            buttons_state,
            model,
        })
    }

    pub fn handle_pairing_events(&mut self) -> Anyhow<PairingState> {
        let events = self.device.fetch_events()?;
        for event in events {
            self.buttons_state.handle_event(event, &self.model)
        }

        Ok(self.get_pairing_state())
    }

    fn get_pairing_state(&self) -> PairingState {
        match self.model {
            Model::LeftJoycon => {
                if self.buttons_state.l ^ self.buttons_state.zl != 0 {
                    PairingState::Waiting(self.get_model())
                } else if self.buttons_state.sl != 0 && self.buttons_state.sr != 0 {
                    PairingState::Horizontal
                } else if self.buttons_state.l != 0 && self.buttons_state.zl != 0 {
                    PairingState::Lone
                } else {
                    PairingState::Pairing
                }
            }
            Model::RightJoycon => {
                if self.buttons_state.r ^ self.buttons_state.zr != 0 {
                    PairingState::Waiting(self.get_model())
                } else if self.buttons_state.sl != 0 && self.buttons_state.sr != 0 {
                    PairingState::Horizontal
                } else if self.buttons_state.r != 0 && self.buttons_state.zr != 0 {
                    PairingState::Lone
                } else {
                    PairingState::Pairing
                }
            }
        }
    }

    pub fn get_model(&self) -> Model {
        self.model
    }
}

impl AsRef<Device> for Controller {
    fn as_ref(&self) -> &Device {
        &self.device
    }
}

impl AsMut<Device> for Controller {
    fn as_mut(&mut self) -> &mut Device {
        &mut self.device
    }
}

impl AsRawFd for Controller {
    fn as_raw_fd(&self) -> std::os::unix::prelude::RawFd {
        self.device.as_raw_fd()
    }
}

impl AsFd for Controller {
    fn as_fd(&self) -> std::os::unix::prelude::BorrowedFd<'_> {
        let raw_fd = self.as_raw_fd();

        // # Safety
        //
        // The fd will remain open until self drops.
        unsafe { BorrowedFd::borrow_raw(raw_fd) }
    }
}

/// To store the button state. This struct only stores pairing-related buttons' state.
#[derive(Default)]
pub struct ButtonsState {
    l: i32,
    r: i32,
    zl: i32,
    zr: i32,
    sl: i32,
    sr: i32,
}

impl ButtonsState {
    fn handle_event(&mut self, ev: evdev::InputEvent, model: &Model) {
        let ev_type = ev.event_type();
        let code = ev.code();
        let key = evdev::Key::new(code);
        let val = ev.value();

        if ev_type != evdev::EventType::KEY {
            return;
        }

        if let Some(state) = model.get_mut_key_state(self, key) {
            *state = val;
        }
    }
}

const LEFT_JOYCON_PRODUCT_ID: u16 = 0x2006;
const RIGHT_JOYCON_PRODUCT_ID: u16 = 0x2007;

#[derive(Copy, Clone, Debug)]
pub enum Model {
    LeftJoycon,
    RightJoycon,
}

impl Model {
    fn get_mut_key_state<'s>(
        &self,
        buttons_state: &'s mut ButtonsState,
        key: evdev::Key,
    ) -> Option<&'s mut i32> {
        match self {
            Model::LeftJoycon => match key {
                evdev::Key::BTN_TL => Some(&mut buttons_state.l),
                evdev::Key::BTN_TL2 => Some(&mut buttons_state.zl),
                evdev::Key::BTN_TR => Some(&mut buttons_state.sl),
                evdev::Key::BTN_TR2 => Some(&mut buttons_state.sr),
                _ => None,
            },
            Model::RightJoycon => match key {
                evdev::Key::BTN_TL => Some(&mut buttons_state.sl),
                evdev::Key::BTN_TL2 => Some(&mut buttons_state.sr),
                evdev::Key::BTN_TR => Some(&mut buttons_state.r),
                evdev::Key::BTN_TR2 => Some(&mut buttons_state.zr),
                _ => None,
            },
        }
    }

    pub fn from_product_id(product_id: u16) -> Anyhow<Self> {
        match product_id {
            LEFT_JOYCON_PRODUCT_ID => Ok(Self::LeftJoycon),
            RIGHT_JOYCON_PRODUCT_ID => Ok(Self::RightJoycon),
            _ => Err(anyhow::anyhow!(
                "Failed to determine the model type for product id {}",
                product_id
            )),
        }
    }

    pub fn is_left(&self) -> bool {
        matches!(self, Self::LeftJoycon)
    }

    pub fn is_right(&self) -> bool {
        matches!(self, Self::RightJoycon)
    }
}

#[derive(Debug)]
pub enum PairingState {
    Pairing,
    Waiting(Model),
    Lone,
    Horizontal,
}
