use std::{
    io,
    os::fd::{AsRawFd, BorrowedFd},
    thread::JoinHandle,
    time::Duration,
};

use compio::runtime::Runtime;
use rustix::event::{PollFd, PollFlags};

use crate::{Adapter, sys::unix::UnixAdapter};

pub struct FuturesAdapter {
    inner: UnixAdapter,
    poll_sender: flume::Sender<Option<Duration>>,
    wait_receiver: flume::Receiver<()>,
    thread: Option<JoinHandle<()>>,
}

impl Adapter for FuturesAdapter {
    fn new(runtime: &Runtime) -> io::Result<Self> {
        let inner = UnixAdapter::new(runtime)?;
        let fd = inner.as_raw_fd();
        let (poll_sender, poll_receiver) = flume::bounded::<Option<Duration>>(1);
        let (wait_sender, wait_receiver) = flume::bounded(1);
        let thread = std::thread::spawn(move || {
            let fd = unsafe { BorrowedFd::borrow_raw(fd) };
            let mut fds = [PollFd::new(&fd, PollFlags::IN)];
            while let Ok(timeout) = poll_receiver.recv() {
                let timeout = timeout.map(|t| t.try_into().unwrap());
                let res = rustix::event::poll(&mut fds, timeout.as_ref());
                match res {
                    Ok(_) => {
                        if wait_sender.send(()).is_err() {
                            break;
                        }
                    }
                    Err(e) => panic!("{:?}", e),
                }
            }
        });
        Ok(Self {
            inner,
            poll_sender,
            wait_receiver,
            thread: Some(thread),
        })
    }

    async fn wait(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.poll_sender
            .send(timeout)
            .expect("cannot send poll request");
        self.wait_receiver
            .recv_async()
            .await
            .expect("polling thread has been dropped");
        Ok(())
    }

    fn clear(&self) -> io::Result<()> {
        self.inner.clear()
    }
}

impl Drop for FuturesAdapter {
    fn drop(&mut self) {
        self.poll_sender.send(None).ok();
        if let Some(thread) = self.thread.take()
            && let Err(e) = thread.join()
        {
            std::panic::resume_unwind(e)
        }
    }
}
