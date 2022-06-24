use std::{
    ffi::{c_void, CString},
    io, ptr,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use libftd2xx::{Ftdi as FtdiBase, FtdiCommon};
use libftd2xx_ffi::{FT_SetEventNotification, FT_EVENT_RXCHAR};
use tokio::sync::mpsc::UnboundedSender;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        Threading::{CreateEventA, SetEvent, WaitForSingleObject},
        WindowsProgramming::INFINITE,
    },
};

use crate::Command;

pub(crate) struct Waker {
    event: HANDLE,
    cancel: Arc<Mutex<bool>>,
    command_tx: UnboundedSender<Command>,
}

pub(crate) struct WakerHandle {
    event: HANDLE,
    cancel: Arc<Mutex<bool>>,
}

impl Waker {
    pub(crate) fn spawn(
        device: &mut FtdiBase,
        command_tx: UnboundedSender<Command>,
    ) -> io::Result<WakerHandle> {
        let handle = device.handle();

        let lpname = CString::new("").unwrap();

        // start event in signalled state to automatically get a first iteration of below loop
        let event = unsafe { CreateEventA(ptr::null(), 0, 1, lpname.as_ptr() as *const u8) };
        unsafe {
            FT_SetEventNotification(handle, FT_EVENT_RXCHAR, event as *mut c_void);
        };
        let cancel = Arc::new(Mutex::new(false));

        thread::spawn({
            let cancel = cancel.clone();
            let command_tx = command_tx.clone();
            move || Self::notifier(cancel, command_tx)
        });

        let waker = Waker {
            cancel: cancel.clone(),
            event,
            command_tx,
        };
        thread::spawn(move || waker.run());
        Ok(WakerHandle { cancel, event })
    }

    fn notifier(cancel: Arc<Mutex<bool>>, tx: UnboundedSender<Command>) {
        loop {
            thread::sleep(Duration::from_millis(20));
            if tx.send(Command::PollRead).is_err() {
                return;
            }
            {
                let lock = cancel.lock().unwrap();
                if *lock {
                    break;
                }
            }
        }
    }

    fn run(self) {
        loop {
            unsafe {
                WaitForSingleObject(self.event, INFINITE);
            }
            log::debug!("Wake-up");
            {
                let lock = self.cancel.lock().unwrap();
                if *lock {
                    break;
                }
            }
            if self.command_tx.send(Command::PollRead).is_err() {
                break;
            }
        }
        let mut lock = self.cancel.lock().unwrap();
        unsafe {
            CloseHandle(self.event);
        }
        *lock = true;
        log::debug!("Waker thread dropped.");
    }
}

impl Drop for WakerHandle {
    fn drop(&mut self) {
        let mut lock = self.cancel.lock().unwrap();
        if !*lock {
            *lock = true;
            unsafe {
                SetEvent(self.event);
            }
        }
    }
}
