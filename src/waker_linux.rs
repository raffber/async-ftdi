use libc::{
    c_int, pthread_cond_destroy, pthread_cond_init, pthread_cond_signal, pthread_cond_t,
    pthread_cond_wait, pthread_mutex_destroy, pthread_mutex_init, pthread_mutex_lock,
    pthread_mutex_t, pthread_mutex_unlock,
};
use libftd2xx::{FtStatus, Ftdi as FtdiBase, FtdiCommon};
use libftd2xx_ffi::{FT_SetEventNotification, FT_EVENT_RXCHAR, FT_STATUS};
use std::{
    ffi::c_void,
    io,
    mem::MaybeUninit,
    ptr::null,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::{status_to_io_error, Command};

pub(crate) struct Waker {
    cancel: Arc<Mutex<bool>>,
    event_handle: Box<EventHandle>,
    command_tx: UnboundedSender<Command>,
}

pub(crate) struct WakerHandle {
    cancel: Arc<Mutex<bool>>,
    cond_var: *mut pthread_cond_t,
}

#[repr(C)]
struct EventHandle {
    e_cond_var: pthread_cond_t,
    e_mutex: pthread_mutex_t,
    i_var: c_int,
}

impl Waker {
    pub(crate) fn spawn(
        device: &mut FtdiBase,
        command_tx: UnboundedSender<Command>,
    ) -> io::Result<WakerHandle> {
        let handle = device.handle();

        let mut cond = MaybeUninit::<pthread_cond_t>::uninit();
        let mut mutex = MaybeUninit::<pthread_mutex_t>::uninit();

        let (cond, mutex) = unsafe {
            let ok = pthread_mutex_init(mutex.as_mut_ptr(), null());
            if ok != 0 {
                return Err(io::Error::last_os_error());
            }
            let ok = pthread_cond_init(cond.as_mut_ptr(), null());
            if ok != 0 {
                return Err(io::Error::last_os_error());
            }
            (cond.assume_init(), mutex.assume_init())
        };

        let mut eh = Box::new(EventHandle {
            e_cond_var: cond,
            e_mutex: mutex,
            i_var: 0,
        });

        let event_mask = FT_EVENT_RXCHAR;
        let status: FT_STATUS = unsafe {
            FT_SetEventNotification(
                handle,
                event_mask,
                eh.as_mut() as *mut EventHandle as *mut c_void,
            )
        };
        if status != 0 {
            unsafe {
                let _ = pthread_cond_destroy(&mut eh.e_cond_var as *mut pthread_cond_t);
                let _ = pthread_mutex_destroy(&mut eh.e_mutex as *mut pthread_mutex_t);
            }
            return Err(status_to_io_error(FtStatus::from(status)));
        }

        let cond_var = &mut eh.e_cond_var as *mut pthread_cond_t;
        let cancel = Arc::new(Mutex::new(false));
        let waker = Waker {
            cancel: cancel.clone(),
            event_handle: eh,
            command_tx: command_tx.clone(),
        };
        thread::spawn({
            let cancel = cancel.clone();
            move || Self::notifier(cancel, command_tx)
        });
        thread::spawn(move || waker.run());

        Ok(WakerHandle { cancel, cond_var })
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

    fn run(mut self) {
        loop {
            unsafe {
                pthread_mutex_lock(&mut self.event_handle.e_mutex as *mut pthread_mutex_t);
                pthread_cond_wait(
                    &mut self.event_handle.e_cond_var as *mut pthread_cond_t,
                    &mut self.event_handle.e_mutex as *mut pthread_mutex_t,
                );
                pthread_mutex_unlock(&mut self.event_handle.e_mutex as *mut pthread_mutex_t);
            }
            log::debug!("Woke-up");
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
            pthread_cond_destroy(&mut self.event_handle.e_cond_var as *mut pthread_cond_t);
            pthread_mutex_destroy(&mut self.event_handle.e_mutex as *mut pthread_mutex_t);
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
                pthread_cond_signal(self.cond_var);
            }
        }
    }
}
