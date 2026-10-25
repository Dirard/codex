//! Concurrency gate of a run that admits `agent()` calls in script order (UC4, R4.1).
//!
//! The script numbers its calls synchronously, but Code Mode delivers host calls on
//! separate tasks, so they can arrive out of order. The gate grants slots strictly by call
//! number; a call that needs no slot passes its number on with [`GateTicket::skip`].

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use tokio::sync::oneshot;

pub(crate) struct OrderedGate {
    state: Arc<Mutex<GateState>>,
}

struct GateState {
    next: u64,
    available: usize,
    waiting: BTreeMap<u64, oneshot::Sender<()>>,
    skipped: BTreeSet<u64>,
}

impl GateState {
    fn pump(&mut self) {
        loop {
            if self.skipped.remove(&self.next) {
                self.next += 1;
                continue;
            }
            if self.available == 0 {
                return;
            }
            let Some(waiter) = self.waiting.remove(&self.next) else {
                return;
            };
            self.next += 1;
            if waiter.send(()).is_ok() {
                self.available -= 1;
            }
        }
    }
}

/// A call's place in the gate, identified by its number in the script.
pub(crate) struct GateTicket {
    state: Arc<Mutex<GateState>>,
    seq: u64,
    settled: bool,
}

/// A held slot; dropping it frees the slot.
pub(crate) struct GatePermit {
    state: Arc<Mutex<GateState>>,
}

impl OrderedGate {
    /// A gate with `slots` concurrent slots for calls numbered from 1.
    pub(crate) fn new(slots: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(GateState {
                next: 1,
                available: slots.max(1),
                waiting: BTreeMap::new(),
                skipped: BTreeSet::new(),
            })),
        }
    }

    pub(crate) fn ticket(&self, seq: u64) -> GateTicket {
        GateTicket {
            state: Arc::clone(&self.state),
            seq,
            settled: false,
        }
    }
}

impl GateTicket {
    /// Wait until every earlier call has passed and a slot is free.
    pub(crate) async fn acquire(mut self) -> GatePermit {
        let (tx, rx) = oneshot::channel();
        {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if self.seq < state.next {
                // A duplicate number cannot keep its order; admit it without blocking others.
                state.available = state.available.saturating_sub(1);
                self.settled = true;
                return GatePermit {
                    state: Arc::clone(&self.state),
                };
            }
            state.waiting.insert(self.seq, tx);
            state.pump();
        }
        self.settled = true;
        let mut pending = PendingGrant {
            state: Arc::clone(&self.state),
            rx: Some(rx),
        };
        if let Some(rx) = pending.rx.as_mut() {
            let _ = rx.await;
        }
        pending.rx = None;
        GatePermit {
            state: Arc::clone(&self.state),
        }
    }

    /// Pass this call's turn on without taking a slot.
    pub(crate) fn skip(mut self) {
        self.settle_without_slot();
    }

    fn settle_without_slot(&mut self) {
        if self.settled {
            return;
        }
        self.settled = true;
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if self.seq >= state.next {
            state.skipped.insert(self.seq);
            state.pump();
        }
    }
}

/// Returns a slot granted to an `acquire` that was cancelled before it took the slot.
struct PendingGrant {
    state: Arc<Mutex<GateState>>,
    rx: Option<oneshot::Receiver<()>>,
}

impl Drop for PendingGrant {
    fn drop(&mut self) {
        if let Some(mut rx) = self.rx.take()
            && rx.try_recv().is_ok()
        {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            state.available += 1;
            state.pump();
        }
    }
}

impl Drop for GateTicket {
    fn drop(&mut self) {
        self.settle_without_slot();
    }
}

impl Drop for GatePermit {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.available += 1;
        state.pump();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    async fn granted(rx: &mut tokio::sync::mpsc::UnboundedReceiver<u64>) -> Vec<u64> {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut seen = Vec::new();
        while let Ok(seq) = rx.try_recv() {
            seen.push(seq);
        }
        seen
    }

    /// R4.1: out-of-order arrivals are admitted in script order, two at a time.
    #[tokio::test]
    async fn admits_in_call_order_within_the_limit() {
        let gate = Arc::new(OrderedGate::new(2));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (release_tx, _) = tokio::sync::broadcast::channel::<()>(8);
        let mut tasks = Vec::new();
        for seq in [3_u64, 5, 1, 4, 2] {
            let gate = Arc::clone(&gate);
            let tx = tx.clone();
            let mut release = release_tx.subscribe();
            tasks.push(tokio::spawn(async move {
                let permit = gate.ticket(seq).acquire().await;
                let _ = tx.send(seq);
                let _ = release.recv().await;
                drop(permit);
            }));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(granted(&mut rx).await, vec![1, 2]);
        let _ = release_tx.send(());
        let mut rest = granted(&mut rx).await;
        let _ = release_tx.send(());
        rest.extend(granted(&mut rx).await);
        let _ = release_tx.send(());
        for task in tasks {
            task.await.expect("task");
        }
        assert_eq!(rest, vec![3, 4, 5]);
    }

    #[tokio::test]
    async fn skipped_and_abandoned_calls_pass_their_turn() {
        let gate = OrderedGate::new(1);
        gate.ticket(1).skip();
        drop(gate.ticket(2));
        let permit = tokio::time::timeout(Duration::from_secs(1), gate.ticket(3).acquire())
            .await
            .expect("call 3 is admitted after 1 and 2 passed");
        drop(permit);
        let _again = tokio::time::timeout(Duration::from_secs(1), gate.ticket(4).acquire())
            .await
            .expect("the slot is free again");
    }
}
