use anyhow::Result as Anyhow;
use polling::{AsRawSource, AsSource, Events, Poller};
use std::collections::HashMap;

use crate::key_allocator::KeyAllocator;

pub const KEY_CAPACITY: usize = 0x100;

pub trait PollCallback<Ctx, Message> {
    fn call(&mut self, ctx: &mut Ctx) -> Message;
}

impl<Ctx, Message, F> PollCallback<Ctx, Message> for F
where
    F: FnMut(&mut Ctx) -> Message + 'static,
{
    fn call(&mut self, ctx: &mut Ctx) -> Message {
        self(ctx)
    }
}

pub struct PollManager<Ctx, Message> {
    poller: Poller,
    callback_map: HashMap<usize, Box<dyn PollCallback<Ctx, Message>>>,
    callback_key_allocator: KeyAllocator,
}

#[allow(unused)]
impl<Ctx, Message> PollManager<Ctx, Message> {
    pub fn new() -> Anyhow<Self> {
        Ok(Self {
            poller: Poller::new()?,
            callback_map: HashMap::new(),
            callback_key_allocator: KeyAllocator::new(KEY_CAPACITY),
        })
    }

    pub fn poll(&mut self, ctx: &mut Ctx) -> Anyhow<Vec<Anyhow<(usize, Message)>>> {
        let mut events = Events::new();
        let _ = self.poller.wait(&mut events, None)?;

        Ok(events
            .iter()
            .map(|event| {
                let key = event.key;
                self.callback_map
                    .get_mut(&key)
                    .ok_or_else(|| {
                        anyhow::anyhow!("Failed to find the corresponding callback for key {}", key)
                    })
                    .map(|callback| callback.call(ctx))
                    .map(|msg| (key, msg))
            })
            .collect())
    }

    /// Subscribe a event.
    pub fn subscribe(
        &mut self,
        source: impl AsRawSource,
        mut event: polling::Event,
        mode: polling::PollMode,
        callback: Box<dyn PollCallback<Ctx, Message>>,
    ) -> Anyhow<usize> {
        let key = self.callback_key_allocator.allocate()?;
        event.key = key;
        unsafe {
            self.poller.add_with_mode(source, event, mode)?;
        }
        self.callback_map.insert(key, callback);

        Ok(key)
    }

    /// Subscribe a event with given key.
    pub fn subscribe_with_key(
        &mut self,
        key: usize,
        source: impl AsRawSource,
        mut event: polling::Event,
        mode: polling::PollMode,
        callback: Box<dyn PollCallback<Ctx, Message>>,
    ) -> Anyhow<()> {
        event.key = key;
        unsafe {
            self.poller.add_with_mode(source, event, mode)?;
        }
        self.callback_map.insert(key, callback);
        self.callback_key_allocator.occupy(key)?;

        Ok(())
    }

    /// Remove a scubscribtion.
    pub fn remove(&mut self, key: usize, source: impl AsSource) -> Anyhow<()> {
        self.callback_map.remove(&key);
        self.poller.delete(source)?;

        Ok(())
    }

    /// Modify a subcribtion.
    pub fn modify(
        &mut self,
        key: usize,
        source: impl AsSource,
        event: polling::Event,
        mode: polling::PollMode,
        callback: Box<dyn PollCallback<Ctx, Message>>,
    ) -> Anyhow<()> {
        self.poller.modify_with_mode(source, event, mode)?;
        self.callback_map.insert(key, callback);
        Ok(())
    }
}
