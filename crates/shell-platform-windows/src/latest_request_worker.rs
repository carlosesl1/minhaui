#![deny(unsafe_code)]

use std::io;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

struct WorkerState<R> {
    pending: Option<R>,
    stopping: bool,
}

struct SharedState<R> {
    state: Mutex<WorkerState<R>>,
    wake: Condvar,
}

pub(crate) struct LatestRequestWorker<R> {
    shared: Arc<SharedState<R>>,
    thread: Option<JoinHandle<()>>,
}

impl<R: Send + 'static> LatestRequestWorker<R> {
    pub(crate) fn spawn(
        name: &str,
        mut execute: impl FnMut(R) + Send + 'static,
    ) -> io::Result<Self> {
        let shared = Arc::new(SharedState {
            state: Mutex::new(WorkerState {
                pending: None,
                stopping: false,
            }),
            wake: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || worker_loop(&worker_shared, &mut execute))?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }

    pub(crate) fn submit(&self, request: R) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.stopping {
            return;
        }
        state.pending = Some(request);
        self.shared.wake.notify_one();
    }
}

impl<R> Drop for LatestRequestWorker<R> {
    fn drop(&mut self) {
        {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.stopping = true;
            state.pending = None;
            self.shared.wake.notify_one();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn worker_loop<R>(shared: &SharedState<R>, execute: &mut impl FnMut(R)) {
    loop {
        let request = {
            let mut state = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            while state.pending.is_none() && !state.stopping {
                state = shared
                    .wake
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            if state.stopping {
                return;
            }
            state.pending.take()
        };
        if let Some(request) = request {
            execute(request);
        }
    }
}
