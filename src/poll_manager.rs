use anyhow::Result as Anyhow;
use polling::{AsRawSource, AsSource, Events, Poller};
use std::collections::HashMap;

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
}

#[allow(unused)]
impl<Ctx, Message> PollManager<Ctx, Message> {
    pub fn new() -> Anyhow<Self> {
        Ok(Self {
            poller: Poller::new()?,
            callback_map: HashMap::new(),
        })
    }

    pub fn poll(&mut self, ctx: &mut Ctx) -> Anyhow<Vec<Anyhow<(usize, Message)>>> {
        let mut events = Events::new();
        let _ = self.poller.wait(&mut events, None)?;

        Ok(events
            .iter()
            .map(|event| {
                let key = event.key;
                // FIXME: We maybe have already remove the callback.
                let callback = self.callback_map.get_mut(&key).unwrap();
                Ok((key, callback.call(ctx)))
            })
            .collect())
    }

    /// Subscribe a event.
    pub fn subscribe(
        &mut self,
        key: usize,
        source: impl AsRawSource,
        event: polling::Event,
        mode: polling::PollMode,
        callback: Box<dyn PollCallback<Ctx, Message>>,
    ) -> Anyhow<()> {
        unsafe {
            self.poller.add_with_mode(source, event, mode)?;
        }
        self.callback_map.insert(key, callback);

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
