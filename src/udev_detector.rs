use anyhow::{Context, Result as Anyhow};
use udev::{Device, Enumerator, MonitorBuilder, MonitorSocket};

use crate::{
    controller_manager::{ControllerManager, ControllerMessage},
    poll_manager::PollCallback,
};

pub struct JoyconUdevDetector;

const JOYCOMBINERD_TAG: &str = "joycombinerd";

impl JoyconUdevDetector {
    pub fn enumerate() -> Anyhow<Vec<Device>> {
        let mut enumerator =
            Enumerator::new().with_context(|| "Failed to create a udev enumerator")?;
        enumerator
            .match_tag(JOYCOMBINERD_TAG)
            .with_context(|| "Failed to add a tag filter to the udev enumerator")?;
        let devices = enumerator.scan_devices()?.collect();
        Ok(devices)
    }

    pub fn monitor() -> Anyhow<MonitorSocket> {
        MonitorBuilder::new()
            .with_context(|| "Failed to create a udev monitor")?
            .match_tag(JOYCOMBINERD_TAG)
            .with_context(|| "Failed to add a tag filter to the udev monitor")?
            .listen()
            .with_context(|| "Failed to listen to the udev monitor")
    }

    fn process_monitor(monitor: &mut MonitorSocket) -> Anyhow<ControllerMessage> {
        monitor
            .iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Failed to get an event from the udev monitor"))
            .map(ControllerMessage::UdevEvent)
    }

    pub fn callback(monitor: MonitorSocket) -> DetectorCallback {
        DetectorCallback::new(monitor)
    }
}

pub struct DetectorCallback {
    monitor: MonitorSocket,
}

impl DetectorCallback {
    pub fn new(monitor: MonitorSocket) -> Self {
        Self { monitor }
    }
}

impl PollCallback<ControllerManager, Anyhow<ControllerMessage>> for DetectorCallback {
    fn call(&mut self, _ctx: &mut ControllerManager) -> Anyhow<ControllerMessage> {
        JoyconUdevDetector::process_monitor(&mut self.monitor)
    }
}
