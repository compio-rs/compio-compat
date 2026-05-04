use std::{io, time::Duration};

use compio::runtime::Runtime;
use compio_log::error;
use mod_use::mod_use;

mod_use!(sys);

pub struct RuntimeCompat<A> {
    runtime: Runtime,
    adapter: A,
}

impl<A: sys::Adapter> RuntimeCompat<A> {
    pub fn new(runtime: Runtime) -> io::Result<Self> {
        let adapter = A::new(&runtime)?;
        Ok(Self { runtime, adapter })
    }

    pub fn adapter(&self) -> &A {
        &self.adapter
    }

    pub fn enter<T, F: FnOnce() -> T>(&self, f: F) -> T {
        self.runtime.enter(f)
    }

    pub async fn enter_async<F: Future>(&self, f: F) -> F::Output {
        let mut f = std::pin::pin!(f);
        std::future::poll_fn(|cx| self.enter(|| f.as_mut().poll(cx))).await
    }

    pub async fn run(&self) {
        self.runtime.poll_with(Some(Duration::ZERO));

        let remaining_tasks = self.runtime.run();

        let timeout = if remaining_tasks {
            Some(Duration::ZERO)
        } else {
            self.runtime.current_timeout()
        };

        self.adapter
            .wait(timeout)
            .await
            .expect("failed to wait for driver");

        if let Err(_e) = self.adapter.clear() {
            error!("failed to clear notifier: {_e:?}");
        }
    }
}
