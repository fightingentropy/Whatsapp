//! Bounded decoder mailbox. Waiting producers sleep on a condition variable.
//! A timeout also releases the egui context if its window disappears: the
//! worker's repaint handle must not keep an abandoned receiver alive forever.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, mpsc::TryRecvError};
use std::time::Duration;

use super::Update;

#[derive(Default)]
struct State {
    frames: VecDeque<Update>,
    receiver_gone: bool,
    sender_gone: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    space: Condvar,
}

pub(super) struct Sender(Arc<Shared>);
pub(super) struct Receiver(Arc<Shared>);

pub(super) fn channel() -> (Sender, Receiver) {
    let shared = Arc::new(Shared::default());
    (Sender(shared.clone()), Receiver(shared))
}

impl Sender {
    pub(super) fn send(&self, update: Update) -> Option<()> {
        self.send_with_timeout(update, Duration::from_secs(5))
    }

    fn send_with_timeout(&self, update: Update, timeout: Duration) -> Option<()> {
        let state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        let (mut state, _) = self
            .0
            .space
            .wait_timeout_while(state, timeout, |state| {
                state.frames.len() >= super::QUEUED_FRAMES && !state.receiver_gone
            })
            .unwrap_or_else(|p| p.into_inner());
        if state.receiver_gone || state.frames.len() >= super::QUEUED_FRAMES {
            state.receiver_gone = true;
            return None;
        }
        state.frames.push_back(update);
        Some(())
    }
}

impl Receiver {
    pub(super) fn try_recv(&self) -> Result<Update, TryRecvError> {
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(update) = state.frames.pop_front() {
            self.0.space.notify_one();
            Ok(update)
        } else if state.sender_gone {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        self.0
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .sender_gone = true;
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        state.receiver_gone = true;
        state.frames.clear();
        self.0.space.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_abandoned_context_cannot_hold_a_decoder_slot_forever() {
        let (sender, receiver) = channel();
        for _ in 0..super::super::QUEUED_FRAMES {
            sender.send(Update::Reset).unwrap();
        }
        assert!(
            sender
                .send_with_timeout(Update::Reset, Duration::from_millis(1))
                .is_none()
        );
        receiver.try_recv().unwrap();
        assert!(
            sender.send(Update::Reset).is_none(),
            "timeout cancels subsequent fallback work too"
        );
        drop(receiver);
        assert!(sender.send(Update::Reset).is_none());
    }
}
