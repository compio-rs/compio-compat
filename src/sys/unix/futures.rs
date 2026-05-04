use std::{
    io,
    os::fd::{AsRawFd, BorrowedFd, OwnedFd},
    thread::JoinHandle,
    time::Duration,
};

use compio::runtime::Runtime;
use rustix::event::{PollFd, PollFlags};

use crate::{Adapter, sys::unix::UnixAdapter};

pub struct FuturesAdapter {
    inner: UnixAdapter,
    pipe: OwnedFd,
    poll_sender: flume::Sender<Option<Duration>>,
    wait_receiver: flume::Receiver<()>,
    thread: Option<JoinHandle<()>>,
}

impl Adapter for FuturesAdapter {
    fn new(runtime: &Runtime) -> io::Result<Self> {
        let inner = UnixAdapter::new(runtime)?;
        let fd = inner.as_raw_fd();

        let (pipe_read, pipe_write) = mk_pipe()?;

        let (poll_sender, poll_receiver) = flume::bounded::<Option<Duration>>(1);
        let (wait_sender, wait_receiver) = flume::bounded(1);
        let thread = std::thread::spawn(move || {
            let fd = unsafe { BorrowedFd::borrow_raw(fd) };
            let mut fds = [
                PollFd::new(&fd, PollFlags::IN),
                PollFd::new(&pipe_read, PollFlags::IN),
            ];
            while let Ok(timeout) = poll_receiver.recv() {
                let timeout = timeout.map(|t| t.try_into().unwrap());
                let res = rustix::event::poll(&mut fds, timeout.as_ref());
                match res {
                    Ok(_) => {
                        if !fds[1].revents().is_empty() {
                            break;
                        }
                        fds[0].clear_revents();
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
            pipe: pipe_write,
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
        if rustix::io::write(&self.pipe, &[0]).is_ok()
            && let Some(thread) = self.thread.take()
            && let Err(e) = thread.join()
        {
            std::panic::resume_unwind(e)
        }
    }
}

#[cfg(not(target_vendor = "apple"))]
pub fn mk_pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    use rustix::pipe::{PipeFlags, pipe_with};

    Ok(pipe_with(PipeFlags::CLOEXEC | PipeFlags::NONBLOCK)?)
}

#[cfg(target_vendor = "apple")]
pub fn mk_pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    use rustix::{fs::*, io::*, pipe::pipe};

    let (a, b) = pipe()?;

    fn set_cloexec(fd: &OwnedFd) -> Result<()> {
        fcntl_setfd(fd, fcntl_getfd(fd)? | FdFlags::CLOEXEC)
    }

    fn set_nonblock(fd: &OwnedFd) -> Result<()> {
        fcntl_setfl(fd, fcntl_getfl(fd)? | OFlags::NONBLOCK)
    }

    set_cloexec(&a)?;
    set_cloexec(&b)?;
    set_nonblock(&a)?;
    set_nonblock(&b)?;

    Ok((a, b))
}
