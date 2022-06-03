use std::io;

use libftd2xx::FtStatus;
use libftd2xx::Ftdi as FtdiBase;
use libftd2xx::FtdiCommon;
use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender},
    oneshot,
};
use waker::Waker;
use waker::WakerHandle;

mod waker;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum StopBits {
    One,
    Two,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Parity {
    None,
    Odd,
    Even,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum DataBits {
    Five,
    Six,
    Seven,
    Eight,
}
pub struct SerialParams {
    pub baud: u32,
    pub data_bits: DataBits,
    pub stop_bits: StopBits,
    pub parity: Parity,
}

pub fn status_to_io_error(status: FtStatus) -> io::Error {
    io::Error::new(io::ErrorKind::Other, status.to_string())
}

pub struct Ftdi {}

impl Ftdi {
    async fn open(serial_number: String, params: &SerialParams) -> io::Result<Ftdi> {
        todo!()
    }

    async fn close(self) -> io::Result<()> {
        todo!()
    }
}

enum Command {
    Cancel,
    PollRead,
    Send(Vec<u8>),
}

struct Event(io::Result<Vec<u8>>);

struct Handler {
    command_tx: UnboundedSender<Command>,
    command_rx: UnboundedReceiver<Command>,
    event_tx: UnboundedSender<Event>,
    waker: WakerHandle,
    device: FtdiBase,
}

impl Handler {
    fn run(
        serial_number: String,
        params: SerialParams,
        open_channel: oneshot::Sender<io::Result<()>>,
        command_tx: UnboundedSender<Command>,
        command_rx: UnboundedReceiver<Command>,
        event_tx: UnboundedSender<Event>,
    ) {
        let device = match FtdiBase::with_serial_number(&serial_number) {
            Ok(device) => {
                let _ = open_channel.send(Ok(()));
                device
            }
            Err(status) => {
                let _ = open_channel.send(Err(status_to_io_error(status)));
                return;
            }
        };

        let waker = Waker::spawn(&mut device, command_tx);

        let mut this = Handler {
            command_tx,
            command_rx,
            event_tx,
            device,
            waker,
        };

        this.run_loop();
        let _ = this.device.close();
    }

    fn run_loop(&mut self) {
        while let Some(msg) = self.command_rx.blocking_recv() {
            match msg {
                Command::Cancel => break,
                Command::PollRead => {
                    // TODO: readd all that's available, push it to the channel
                }
                Command::Send(data) => {
                    // TODO: push everything to device
                }
            }
        }
    }
}
