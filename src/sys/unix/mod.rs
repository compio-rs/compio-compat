#[cfg(target_os = "linux")]
use std::os::fd::{BorrowedFd, OwnedFd};
use std::{
    io,
    os::fd::{AsFd, AsRawFd, RawFd},
};

use compio::runtime::Runtime;
use mod_use::mod_use;

#[cfg(feature = "tokio")]
mod_use![tokio];

#[cfg(feature = "futures")]
mod_use![futures];

#[cfg(feature = "smol")]
mod_use![smol];

struct UnixAdapter {
    driver: RawFd,
    #[cfg(target_os = "linux")]
    efd: Option<OwnedFd>,
}

#[cfg(target_os = "linux")]
impl UnixAdapter {
    fn new(runtime: &Runtime) -> io::Result<Self> {
        let driver = runtime.as_raw_fd();
        if runtime.driver_type().is_iouring() {
            use rustix::{
                event::{EventfdFlags, eventfd},
                io_uring::{IoringRegisterOp, io_uring_register},
            };

            let efd = eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)?;
            let efd_raw = efd.as_raw_fd();
            unsafe {
                io_uring_register(
                    BorrowedFd::borrow_raw(driver),
                    IoringRegisterOp::RegisterEventfd,
                    (&raw const efd_raw).cast(),
                    1,
                )?;
            }
            Ok(Self {
                driver,
                efd: Some(efd),
            })
        } else {
            Ok(Self { driver, efd: None })
        }
    }

    fn clear(&self) -> io::Result<()> {
        if let Some(efd) = &self.efd {
            let mut buf = [0u8; 8];
            rustix::io::read(efd, &mut buf)?;
        }
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
impl UnixAdapter {
    fn new(runtime: &Runtime) -> io::Result<Self> {
        let driver = runtime.as_raw_fd();
        Ok(Self { driver })
    }

    fn clear(&self) -> io::Result<()> {
        Ok(())
    }
}

impl AsRawFd for UnixAdapter {
    fn as_raw_fd(&self) -> RawFd {
        #[cfg(target_os = "linux")]
        {
            self.efd
                .as_ref()
                .map(|f| f.as_raw_fd())
                .unwrap_or(self.driver)
        }
        #[cfg(not(target_os = "linux"))]
        {
            self.driver
        }
    }
}

impl AsFd for UnixAdapter {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.as_raw_fd()) }
    }
}
