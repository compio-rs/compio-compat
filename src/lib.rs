use std::{
    io,
    ops::Deref,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use compio::runtime::Runtime;
use compio_log::error;
use mod_use::mod_use;
use pin_project_lite::pin_project;

mod_use!(sys);

pub struct RuntimeCompat<A> {
    runtime: A,
}

impl<A: sys::Adapter> RuntimeCompat<A> {
    pub fn new(runtime: Runtime) -> io::Result<Self> {
        let runtime = A::new(runtime)?;
        Ok(Self { runtime })
    }

    pub async fn execute<F: Future>(&self, f: F) -> F::Output {
        let waker = self.runtime.waker();
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(f);
        loop {
            if let Poll::Ready(result) = self.runtime.enter(|| future.as_mut().poll(&mut context)) {
                self.runtime.enter(|| self.runtime.run());
                return result;
            }

            let remaining_tasks = self.runtime.enter(|| self.runtime.run());

            let timeout = if remaining_tasks {
                Some(Duration::ZERO)
            } else {
                self.runtime.current_timeout()
            };

            self.runtime.submit();

            match self.runtime.wait(timeout).await {
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => panic!("failed to wait for driver: {e:?}"),
            }

            if let Err(_e) = self.runtime.clear() {
                error!("failed to clear notifier: {_e:?}");
            }

            self.runtime.poll_with(Some(Duration::ZERO));
        }
    }
}

impl<A: sys::Adapter> Deref for RuntimeCompat<A> {
    type Target = Runtime;

    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

pin_project! {
    pub struct EnterAsync<F: ?Sized> {
        runtime: Runtime,
        #[pin]
        future: F,
    }
}

impl<F: Future + ?Sized> Future for EnterAsync<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        this.runtime.enter(|| this.future.poll(cx))
    }
}
