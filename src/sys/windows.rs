use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle},
    thread::JoinHandle,
    time::Duration,
};

use compio::{driver::AsRawFd, runtime::Runtime};
use compio_log::trace;
use windows_sys::Win32::{
    Foundation::{WAIT_FAILED, WAIT_OBJECT_0},
    System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects},
};

use crate::sys::Adapter;

struct WindowsAdapter {
    event: OwnedHandle,
    poll_sender: flume::Sender<Option<Duration>>,
    wait_receiver: flume::Receiver<()>,
    thread: Option<JoinHandle<()>>,
}

impl Adapter for WindowsAdapter {
    fn new(runtime: &Runtime) -> io::Result<Self> {
        let iocp = runtime.as_raw_fd();
        let event = unsafe { CreateEventW(std::ptr::null_mut(), 0, 0, std::ptr::null()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        let event = unsafe { OwnedHandle::from_raw_handle(event) };
        let (poll_sender, poll_receiver) = flume::bounded::<Option<Duration>>(1);
        let (wait_sender, wait_receiver) = flume::bounded(1);
        let event_handle = event.as_raw_handle();
        let thread = std::thread::spawn({
            let iocp = iocp as usize;
            let event_handle = event_handle as usize;
            move || {
                while let Ok(timeout) = poll_receiver.recv() {
                    trace!("polling with timeout: {:?}", timeout);
                    let timeout = match timeout {
                        Some(timeout) => timeout.as_millis() as u32,
                        None => INFINITE,
                    };
                    let handles = [event_handle as RawHandle, iocp as RawHandle];
                    let res = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, timeout) };
                    match res {
                        WAIT_OBJECT_0 => break,
                        WAIT_FAILED => panic!("{:?}", io::Error::last_os_error()),
                        _ => {
                            if wait_sender.send(()).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        });
        Ok(Self {
            event,
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
        Ok(())
    }
}

impl Drop for WindowsAdapter {
    fn drop(&mut self) {
        self.poll_sender.send(None).ok();
        let res = unsafe { SetEvent(self.event.as_raw_handle()) };
        if res != 0
            && let Some(thread) = self.thread.take()
            && let Err(e) = thread.join()
        {
            std::panic::resume_unwind(e)
        }
    }
}

macro_rules! impl_adapter {
    ($name:ident) => {
        pub struct $name(WindowsAdapter);

        impl Adapter for $name {
            fn new(runtime: &Runtime) -> io::Result<Self> {
                WindowsAdapter::new(runtime).map(Self)
            }

            async fn wait(&self, timeout: Option<Duration>) -> io::Result<()> {
                self.0.wait(timeout).await
            }

            fn clear(&self) -> io::Result<()> {
                self.0.clear()
            }
        }
    };
}

#[cfg(feature = "futures")]
impl_adapter!(FuturesAdapter);

#[cfg(feature = "tokio")]
impl_adapter!(TokioAdapter);

#[cfg(feature = "smol")]
impl_adapter!(SmolAdapter);
