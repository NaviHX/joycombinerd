use std::{cell::RefCell, collections::HashMap, rc::Rc};

use anyhow::{Context, Result as Anyhow};
use evdev::{
    uinput::{UInputEvent, VirtualDevice}, AbsInfo, AttributeSet, EventType, FFEffectType, InputEvent, Key, UInputEventType, UinputAbsSetup
};

use super::controller::Controller;

type KeyMap = HashMap<(usize, u16, u16), (EventType, u16, &'static dyn Fn(i32) -> i32)>;

const ABSINFO_VALUE: i32 = 0;
const ABSINFO_MIN: i32 = -32767;
const ABSINFO_MAX: i32 = 32767;
const ABSINFO_FUZZ: i32 = 250;
const ABSINFO_FLAT: i32 = 500;
const ABSINFO_RESOLUTION: i32 = 0;

pub struct VirtualController {
    virtual_device: VirtualDevice,
    physical_devices: Vec<Rc<RefCell<Controller>>>,
    key_map: KeyMap,
    rumble_effects: HashMap<u16, ()>,
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
                let original_type = event.event_type().0;
                let original_val = event.value();

                self.key_map
                    .get(&(physical_device_id, original_type, original_code))
                    .map(|&(relay_type, relay_code, f)| {
                        InputEvent::new(relay_type, relay_code, f(original_val))
                    })
            })
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
                    let (ff_l, ff_r) = (event, event);

                    #[allow(clippy::get_first)]
                    if let Some(phys_dev) = self.physical_devices.get(0) {
                        phys_dev.borrow_mut().as_mut().send_events(&[ff_l]).with_context(|| "Failed to forward the rumble data to the left controller")?;
                    }
                    if let Some(phys_dev) = self.physical_devices.get(1) {
                        phys_dev.borrow_mut().as_mut().send_events(&[ff_r]).with_context(|| "Failed to forward the rumble data to the right controller")?;
                    }
                }

                EventType::UINPUT => {
                    match UInputEventType(event.code()) {
                        UInputEventType::UI_FF_UPLOAD => {
                            let upload = self.virtual_device.process_ff_upload(event).with_context(|| "Failed to process the ff upload")?;
                            let effect = upload.effect();
                            let id = upload.effect_id();

                            todo!()
                        }
                        UInputEventType::UI_FF_ERASE => {
                            todo!()
                        }
                        _ => Err(anyhow::anyhow!("Unhandled uinput event {event:?}"))?,
                    }
                }

                _ => Err(anyhow::anyhow!("Unhandled event: {event:?}"))?,
            }
        }

        Ok(())
    }

    pub fn new(physical_devices: Vec<Rc<RefCell<Controller>>>, key_map: KeyMap) -> Anyhow<Self> {
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
