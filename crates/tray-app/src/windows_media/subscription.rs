//! Event handlers registered with Windows, removed again when dropped.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;
use windows::Foundation::TypedEventHandler;
use windows::core::RuntimeType;

type Remove = Box<dyn FnOnce() -> windows::core::Result<()> + Send>;

/// One registered event handler. Dropping it removes the handler.
pub struct Subscription {
    event: &'static str,
    remove: Option<Remove>,
}

impl Subscription {
    /// Registers a handler with `add` and keeps `remove` for the drop. Both
    /// get the registration token Windows hands out.
    pub fn register<Source>(
        event: &'static str,
        source: &Source,
        add: impl FnOnce(&Source) -> windows::core::Result<i64>,
        remove: impl FnOnce(&Source, i64) -> windows::core::Result<()> + Send + 'static,
    ) -> windows::core::Result<Self>
    where
        Source: Clone + Send + 'static,
    {
        let token = add(source)?;
        let source = source.clone();
        Ok(Self {
            event,
            remove: Some(Box::new(move || remove(&source, token))),
        })
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(remove) = self.remove.take()
            && let Err(error) = remove()
        {
            // The session may be gone already, which removes its handlers.
            tracing::debug!(event = self.event, %error, "cannot remove an event handler");
        }
    }
}

/// An event handler that only wakes whoever waits on `changed`.
pub fn wake<Sender, Args>(changed: &Arc<Notify>) -> TypedEventHandler<Sender, Args>
where
    Sender: RuntimeType + 'static,
    Args: RuntimeType + 'static,
{
    let changed = Arc::clone(changed);
    TypedEventHandler::new(move |_, _| {
        changed.notify_one();
        Ok(())
    })
}

/// An event handler that sets `flag`, then wakes whoever waits on
/// `changed`, so the reader can tell which kind of event woke it.
pub fn mark_and_wake<Sender, Args>(
    flag: &Arc<AtomicBool>,
    changed: &Arc<Notify>,
) -> TypedEventHandler<Sender, Args>
where
    Sender: RuntimeType + 'static,
    Args: RuntimeType + 'static,
{
    let flag = Arc::clone(flag);
    let changed = Arc::clone(changed);
    TypedEventHandler::new(move |_, _| {
        flag.store(true, Ordering::Release);
        changed.notify_one();
        Ok(())
    })
}
