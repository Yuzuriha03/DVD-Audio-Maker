//! Bounded, ordered execution for independent workflow items.
use dvda_native::media::Callbacks;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

enum Event<R> {
    Log(i32, String),
    Finished(usize, R),
    Failed(String),
}

struct WorkerCallbacks<'a, R> {
    send: &'a mpsc::SyncSender<Event<R>>,
    stop: &'a AtomicBool,
}

impl<R> Callbacks for WorkerCallbacks<'_, R> {
    fn emit(&mut self, stream: i32, text: &str) {
        if self.send.send(Event::Log(stream, text.into())).is_err() {
            self.stop.store(true, Ordering::Release);
        }
    }

    fn cancelled(&mut self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
}

/// Run independent items on the automatic worker pool. Results retain input
/// order, while logs and completion notifications stay on the caller thread.
pub(crate) fn map<T: Sync, R: Send>(
    items: &[T],
    caller: &mut dyn Callbacks,
    cancelled_message: &str,
    operation: impl Fn(usize, &T, &mut dyn Callbacks) -> R + Sync,
    finished: impl FnMut(usize, &R, &mut dyn Callbacks),
) -> Result<Vec<R>, String> {
    map_with_workers(
        items,
        crate::options::worker_count(items.len()),
        caller,
        cancelled_message,
        operation,
        finished,
    )
}

pub(crate) fn map_with_workers<T: Sync, R: Send>(
    items: &[T],
    workers: usize,
    caller: &mut dyn Callbacks,
    cancelled_message: &str,
    operation: impl Fn(usize, &T, &mut dyn Callbacks) -> R + Sync,
    mut finished: impl FnMut(usize, &R, &mut dyn Callbacks),
) -> Result<Vec<R>, String> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let workers = workers.max(1).min(items.len());
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let (send, receive) = mpsc::sync_channel(workers * 2);
    let mut results: Vec<_> = (0..items.len()).map(|_| None).collect();
    let mut cancelled = false;
    let mut failure = None;
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let send = send.clone();
            let next = &next;
            let stop = &stop;
            let operation = &operation;
            scope.spawn(move || {
                let mut callbacks = WorkerCallbacks { send: &send, stop };
                let result = catch_unwind(AssertUnwindSafe(|| {
                    while !callbacks.cancelled() {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            break;
                        };
                        let result = operation(index, item, &mut callbacks);
                        if send.send(Event::Finished(index, result)).is_err() {
                            break;
                        }
                    }
                }));
                if result.is_err() {
                    stop.store(true, Ordering::Release);
                    let _ = send.send(Event::Failed("Parallel worker panicked".into()));
                }
            });
        }
        drop(send);
        let callbacks = catch_unwind(AssertUnwindSafe(|| {
            loop {
                if caller.cancelled() {
                    cancelled = true;
                    stop.store(true, Ordering::Release);
                }
                match receive.recv_timeout(Duration::from_millis(20)) {
                    Ok(Event::Log(stream, text)) => caller.emit(stream, &text),
                    Ok(Event::Finished(index, result)) => {
                        finished(index, &result, caller);
                        results[index] = Some(result);
                    }
                    Ok(Event::Failed(error)) => {
                        failure.get_or_insert(error);
                        stop.store(true, Ordering::Release);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        }));
        if callbacks.is_err() {
            stop.store(true, Ordering::Release);
            while receive.recv().is_ok() {}
            failure = Some("Parallel callback panicked".into());
        }
    });
    if cancelled {
        return Err(cancelled_message.into());
    }
    if let Some(error) = failure {
        return Err(error);
    }
    results
        .into_iter()
        .map(|result| result.ok_or_else(|| "Parallel worker returned no result".into()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Events {
        thread: Option<std::thread::ThreadId>,
        completed: Vec<usize>,
        logs: Vec<String>,
        cancel: bool,
    }

    impl Callbacks for Events {
        fn emit(&mut self, _: i32, text: &str) {
            assert_eq!(Some(std::thread::current().id()), self.thread);
            self.logs.push(text.into());
        }

        fn cancelled(&mut self) -> bool {
            self.cancel
        }
    }

    #[test]
    fn preserves_result_order_and_forwards_worker_callbacks() {
        let items = [40, 0, 20, 10];
        let mut events = Events {
            thread: Some(std::thread::current().id()),
            ..Events::default()
        };
        let mut completed = Vec::new();
        let results = map(
            &items,
            &mut events,
            "cancelled",
            |_, item, callbacks| {
                for _ in 0..*item {
                    std::thread::yield_now();
                }
                callbacks.emit(1, &format!("finished {item}"));
                *item
            },
            |index, _, _| completed.push(index),
        )
        .unwrap();
        events.completed = completed;
        assert_eq!(results, items);
        assert_eq!(events.completed.len(), items.len());
        assert_eq!(events.logs.len(), items.len());
    }

    #[test]
    fn cancellation_stops_new_work() {
        let items = [0; 32];
        let mut events = Events {
            thread: Some(std::thread::current().id()),
            cancel: true,
            ..Events::default()
        };
        let error = map(
            &items,
            &mut events,
            "cancelled fixture",
            |_, _, _| (),
            |_, _, _| {},
        )
        .unwrap_err();
        assert_eq!(error, "cancelled fixture");
    }
}
