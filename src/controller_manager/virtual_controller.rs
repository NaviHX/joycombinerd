use std::{
    cell::RefCell,
    collections::HashMap,
    os::fd::{AsFd, AsRawFd, BorrowedFd},
    rc::Rc,
};

use anyhow::{Context, Result as Anyhow};
use evdev::{
    uinput::{FFUploadEvent, UInputEvent, VirtualDevice},
    AbsInfo, AttributeSet, EventType, FFEffect, FFEffectData, FFEffectType, InputEvent, Key,
    UInputEventType, UinputAbsSetup,
};

use super::controller::Controller;

pub trait KeyMap {
    fn map_key(
        &self,
        controller_id: usize,
        event_type: EventType,
        code: u16,
        value: i32,
    ) -> Option<(EventType, u16, i32)>;
}

const ABSINFO_VALUE: i32 = 0;
const ABSINFO_MIN: i32 = -32767;
const ABSINFO_MAX: i32 = 32767;
const ABSINFO_FUZZ: i32 = 250;
const ABSINFO_FLAT: i32 = 500;
const ABSINFO_RESOLUTION: i32 = 0;

pub struct VirtualController {
    virtual_device: VirtualDevice,
    physical_devices: Vec<Rc<RefCell<Controller>>>,
    key_map: Box<dyn KeyMap>,
    rumble_effects: HashMap<u16, (Option<FFEffect>, Option<FFEffect>)>,
}

impl VirtualController {
    pub fn relay_input_events(&mut self, physical_device_id: usize) -> Anyhow<()> {
        let mut physical_device = self
            .physical_devices
            .get(physical_device_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to find physical device {physical_device_id} in the virtual device"
                )
            })?
            .borrow_mut();
        let events = physical_device.as_mut().fetch_events()?;

        let relay_events: Vec<InputEvent> = events
            .flat_map(|event| {
                let original_code = event.code();
                let original_type = event.event_type();
                let original_val = event.value();

                self.key_map.map_key(
                    physical_device_id,
                    original_type,
                    original_code,
                    original_val,
                )
            })
            .map(|(event_type, code, value)| InputEvent::new_now(event_type, code, value))
            .collect();

        self.virtual_device.emit(&relay_events)?;
        Ok(())
    }

    pub fn relay_output_events(&mut self) -> Anyhow<()> {
        // HACK: Only the first two physical devices will receive the rumble command.
        let events: Vec<_> = self.virtual_device.fetch_events()?.collect();
        for event in events {
            match event.event_type() {
                EventType::FORCEFEEDBACK => {
                    let event = unsafe { get_input_event_from_uinput_event(event) };
                    let (ff_l, ff_r) = self
                        .rumble_effects
                        .get(&event.code())
                        .ok_or_else(|| anyhow::anyhow!("No corresponding effects"))?;

                    #[allow(clippy::get_first)]
                    if let Some((phys_dev, ff)) = self
                        .physical_devices
                        .get(0)
                        .and_then(|dev| ff_l.as_ref().map(|ff| (dev, ff)))
                    {
                        let code = ff.id();
                        let ff = InputEvent::new_now(event.event_type(), code, event.value());
                        phys_dev
                            .borrow_mut()
                            .as_mut()
                            .send_events(&[ff])
                            .with_context(|| {
                                "Failed to forward the rumble data to the left controller"
                            })?;
                    }
                    if let Some((phys_dev, ff)) = self
                        .physical_devices
                        .get(1)
                        .and_then(|dev| ff_r.as_ref().map(|ff| (dev, ff)))
                    {
                        let code = ff.id();
                        let ff = InputEvent::new_now(event.event_type(), code, event.value());
                        phys_dev
                            .borrow_mut()
                            .as_mut()
                            .send_events(&[ff])
                            .with_context(|| {
                                "Failed to forward the rumble data to the right controller"
                            })?;
                    }
                }

                EventType::UINPUT => {
                    match UInputEventType(event.code()) {
                        UInputEventType::UI_FF_UPLOAD => {
                            let mut upload = self
                                .virtual_device
                                .process_ff_upload(event)
                                .with_context(|| "Failed to process the ff upload")?;
                            let effect = upload.effect();
                            let id = upload.effect_id();

                            let allocate_new_effect =
                                id == -1 || !self.rumble_effects.contains_key(&(id as u16));

                            if allocate_new_effect {
                                #[allow(clippy::get_first)]
                                let effect_l = if let Some(device) = self.physical_devices.get(0) {
                                    Some(upload_ff_effect(
                                        &mut (**device).borrow_mut(),
                                        effect,
                                        &mut upload,
                                    )?)
                                } else {
                                    None
                                };

                                let effect_r = if let Some(device) = self.physical_devices.get(1) {
                                    Some(upload_ff_effect(
                                        &mut (**device).borrow_mut(),
                                        effect,
                                        &mut upload,
                                    )?)
                                } else {
                                    None
                                };

                                // HACK: use the id from left controller.
                                let id = effect_l.as_ref().ok_or_else(|| anyhow::anyhow!("Virtual Controller don't even have at least one physical device!? 🤯"))?.id();
                                self.rumble_effects.insert(id, (effect_l, effect_r));
                                upload.set_effect_id(id as i16);
                            }
                        }
                        UInputEventType::UI_FF_ERASE => {
                            let mut erase = self
                                .virtual_device
                                .process_ff_erase(event)
                                .with_context(|| "Failed to process the ff erase")?;
                            let id = erase.effect_id() as u16;
                            if self.rumble_effects.contains_key(&id) {
                                self.rumble_effects.remove(&id);
                            } else {
                                erase.set_retval(-1);
                            }
                        }
                        _ => Err(anyhow::anyhow!("Unhandled uinput event {event:?}"))?,
                    }
                }

                _ => Err(anyhow::anyhow!("Unhandled event: {event:?}"))?,
            }
        }

        Ok(())
    }

    pub fn new(
        physical_devices: Vec<Rc<RefCell<Controller>>>,
        key_map: Box<dyn KeyMap>,
    ) -> Anyhow<Self> {
        // HACK: 0x2008 is an illegal product id for nintendo joycons, preventing re-registering
        // the virtual controllers.
        let input_id = evdev::InputId::new(evdev::BusType::BUS_VIRTUAL, 0x059e, 0x2008, 0x0000);
        let mut virtual_device = evdev::uinput::VirtualDeviceBuilder::new()?
            .name("Nintendo Switch Combined Joycons")
            .input_id(input_id);

        let mut keys = AttributeSet::new();
        keys.insert(Key::BTN_SELECT);
        keys.insert(Key::BTN_Z);
        keys.insert(Key::BTN_THUMBL);
        keys.insert(Key::BTN_START);
        keys.insert(Key::BTN_MODE);
        keys.insert(Key::BTN_THUMBR);
        keys.insert(Key::BTN_SOUTH);
        keys.insert(Key::BTN_EAST);
        keys.insert(Key::BTN_NORTH);
        keys.insert(Key::BTN_WEST);
        keys.insert(Key::BTN_DPAD_UP);
        keys.insert(Key::BTN_DPAD_DOWN);
        keys.insert(Key::BTN_DPAD_LEFT);
        keys.insert(Key::BTN_DPAD_RIGHT);
        keys.insert(Key::BTN_TL);
        keys.insert(Key::BTN_TR);
        keys.insert(Key::BTN_TL2);
        keys.insert(Key::BTN_TR2);
        virtual_device = virtual_device
            .with_keys(&keys)
            .with_context(|| "Failed to init keys for the virtual controller")?;

        let absinfo = AbsInfo::new(
            ABSINFO_VALUE,
            ABSINFO_MIN,
            ABSINFO_MAX,
            ABSINFO_FUZZ,
            ABSINFO_FLAT,
            ABSINFO_RESOLUTION,
        );
        virtual_device = virtual_device
            .with_absolute_axis(&UinputAbsSetup::new(
                evdev::AbsoluteAxisType::ABS_X,
                absinfo,
            ))
            .with_context(|| "Failed to init abs for the virtual controller")?;
        virtual_device = virtual_device
            .with_absolute_axis(&UinputAbsSetup::new(
                evdev::AbsoluteAxisType::ABS_Y,
                absinfo,
            ))
            .with_context(|| "Failed to init abs for the virtual controller")?;
        virtual_device = virtual_device
            .with_absolute_axis(&UinputAbsSetup::new(
                evdev::AbsoluteAxisType::ABS_RX,
                absinfo,
            ))
            .with_context(|| "Failed to init abs for the virtual controller")?;
        virtual_device = virtual_device
            .with_absolute_axis(&UinputAbsSetup::new(
                evdev::AbsoluteAxisType::ABS_RY,
                absinfo,
            ))
            .with_context(|| "Failed to init abs for the virtual controller")?;

        let mut ff_effects = AttributeSet::new();
        ff_effects.insert(FFEffectType::FF_RUMBLE);
        ff_effects.insert(FFEffectType::FF_PERIODIC);
        ff_effects.insert(FFEffectType::FF_SQUARE);
        ff_effects.insert(FFEffectType::FF_TRIANGLE);
        ff_effects.insert(FFEffectType::FF_SINE);
        ff_effects.insert(FFEffectType::FF_GAIN);
        virtual_device = virtual_device
            .with_ff(&ff_effects)
            .with_context(|| "Failed to init FF for the virtual controller")?;

        let virtual_device = virtual_device
            .build()
            .with_context(|| "Failed to create the virtual controller")?;

        Ok(Self {
            virtual_device,
            physical_devices,
            key_map,
            rumble_effects: HashMap::new(),
        })
    }
}

impl AsRawFd for VirtualController {
    fn as_raw_fd(&self) -> std::os::unix::prelude::RawFd {
        self.virtual_device.as_raw_fd()
    }
}

impl AsFd for VirtualController {
    fn as_fd(&self) -> std::os::unix::prelude::BorrowedFd<'_> {
        let raw_fd = self.as_raw_fd();

        // # Safety
        //
        // The fd will remain open until self drops.
        unsafe { BorrowedFd::borrow_raw(raw_fd) }
    }
}

/// I need to relay the input events the virtual devices received to the real devices. Anyway I can
/// neither copy or clone the `UInputEvent` struct or get the inner `InputEvent` struct. So I will
/// do some dirty work here. 🤓
///
/// # Safety
///
/// - `event` is a valid `UInputEvent`
/// - `UInputEvent`'s first field is an `InputEvent`
unsafe fn get_input_event_from_uinput_event(event: UInputEvent) -> InputEvent {
    let ptr = &event as *const UInputEvent;
    unsafe { *ptr.cast::<InputEvent>() }
}

fn upload_ff_effect(
    controller: &mut Controller,
    effect: FFEffectData,
    upload: &mut FFUploadEvent,
) -> Anyhow<FFEffect> {
    let res = controller.as_mut().upload_ff_effect(effect);

    if let Err(e) = &res {
        upload.set_retval(e.raw_os_error().unwrap_or(-1));
    }

    res.with_context(|| "Failed to upload the ff effect")
}

pub mod key_map {
    #![allow(unused)]
    use super::KeyMap;

    pub struct Id;

    impl KeyMap for Id {
        fn map_key(
            &self,
            _controller_id: usize,
            event_type: evdev::EventType,
            code: u16,
            value: i32,
        ) -> Option<(evdev::EventType, u16, i32)> {
            Some((event_type, code, value))
        }
    }

    impl Id {
        pub fn new() -> Self {
            Self
        }
    }

    pub type LoneConstrollerKeyMap = Id;
    pub type CombinedControllerKeyMap = Id;

    /// FIXME: Rotate the keymap for horizontal controllers.
    pub type HorizontalLeftControllerKeyMap = Id;
    pub type HorizontalRightControllerKeyMap = Id;
}
