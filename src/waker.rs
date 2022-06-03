use libftd2xx::Ftdi as FtdiBase;
use std::ffi::c_void;
use tokio::sync::mpsc::UnboundedSender;

use crate::Command;

pub(crate) struct Waker {}

pub(crate) struct WakerHandle {}

impl Waker {
    pub(crate) fn spawn(
        device: &mut FtdiBase,
        command_tx: UnboundedSender<Command>,
    ) -> WakerHandle {
        let handle = device.handle();

        WakerHandle {}
    }
}

impl Drop for WakerHandle {
    fn drop(&mut self) {
        todo!()
    }
}
