use libc::{
    c_int, pthread_cond_destroy, pthread_cond_init, pthread_cond_signal, pthread_cond_t,
    pthread_cond_wait, pthread_mutex_destroy, pthread_mutex_init, pthread_mutex_lock,
    pthread_mutex_t, pthread_mutex_unlock,
};
use libftd2xx::{Ftdi as FtdiBase, FtdiCommon};
use libftd2xx_ffi::{FT_SetEventNotification, FT_EVENT_RXCHAR};
use std::{
    ffi::c_void,
    mem::MaybeUninit,
    ptr::null,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::Command;

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
    ) -> WakerHandle {
        let handle = device.handle();

        let mut eh = unsafe {
            Box::new(EventHandle {
                e_cond_var: MaybeUninit::uninit().assume_init(),
                e_mutex: MaybeUninit::uninit().assume_init(),
                i_var: 0,
            })
        };
        unsafe {
            pthread_mutex_init(&mut eh.e_mutex as *mut pthread_mutex_t, null());
            pthread_cond_init(&mut eh.e_cond_var as *mut pthread_cond_t, null());
        };

        let event_mask = FT_EVENT_RXCHAR;
        let status = unsafe {
            FT_SetEventNotification(
                handle,
                event_mask,
                eh.as_mut() as *mut EventHandle as *mut c_void,
            )
        };

        let cond_var = &mut eh.e_cond_var as *mut pthread_cond_t;
        let cancel = Arc::new(Mutex::new(false));
        let waker = Waker {
            cancel: cancel.clone(),
            event_handle: eh,
            command_tx,
        };
        thread::spawn(move || waker.run());

        WakerHandle { cancel, cond_var }
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
    }
}

impl Drop for WakerHandle {
    fn drop(&mut self) {
        let mut lock = self.cancel.lock().unwrap();
        if !*lock {
            unsafe {
                pthread_cond_signal(self.cond_var);
            }
        }
    }
}
