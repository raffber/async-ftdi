use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::io::ErrorKind;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::thread;
use std::time::Duration;

use libftd2xx::list_devices;
use libftd2xx::BitsPerWord;
use libftd2xx::FtStatus;
use libftd2xx::Ftdi as FtdiBase;
use libftd2xx::FtdiCommon;
use libftd2xx::TimeoutError;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender},
    oneshot,
};

#[cfg(target_os = "linux")]
mod waker_linux;

use tokio::task::spawn_blocking;
#[cfg(target_os = "linux")]
use waker_linux::{Waker, WakerHandle};

#[cfg(target_os = "windows")]
mod waker_windows;

#[cfg(target_os = "windows")]
use waker_windows::{Waker, WakerHandle};

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
    Seven,
    Eight,
}

#[derive(Clone, Debug)]
pub struct SerialParams {
    pub baud: u32,
    pub data_bits: DataBits,
    pub stop_bits: StopBits,
    pub parity: Parity,
}

impl From<StopBits> for libftd2xx::StopBits {
    fn from(x: StopBits) -> Self {
        match x {
            StopBits::One => libftd2xx::StopBits::Bits1,
            StopBits::Two => libftd2xx::StopBits::Bits2,
        }
    }
}

impl From<DataBits> for BitsPerWord {
    fn from(x: DataBits) -> Self {
        match x {
            DataBits::Seven => BitsPerWord::Bits7,
            DataBits::Eight => BitsPerWord::Bits8,
        }
    }
}

impl From<Parity> for libftd2xx::Parity {
    fn from(x: Parity) -> Self {
        match x {
            Parity::None => libftd2xx::Parity::No,
            Parity::Odd => libftd2xx::Parity::Odd,
            Parity::Even => libftd2xx::Parity::Even,
        }
    }
}

pub fn status_to_io_error(status: FtStatus) -> io::Error {
    io::Error::new(io::ErrorKind::Other, status.to_string())
}

fn disconnected_error() -> io::Error {
    io::Error::new(ErrorKind::Other, "Channel Disconnected")
}

#[derive(Debug)]
pub struct Ftdi {
    buffer: VecDeque<u8>,
    command_tx: UnboundedSender<Command>,
    event_rx: UnboundedReceiver<Event>,
    error: Option<io::Error>,
    shutdown_rx: Option<oneshot::Receiver<()>>,
}

pub use libftd2xx::DeviceInfo;

impl Ftdi {
    pub async fn list_devices() -> io::Result<Vec<DeviceInfo>> {
        spawn_blocking(|| list_devices().map_err(status_to_io_error))
            .await
            .unwrap()
    }

    pub async fn open(serial_number: &str, params: &SerialParams) -> io::Result<Ftdi> {
        let (open_tx, open_rx) = oneshot::channel();
        let (command_tx, command_rx) = unbounded_channel();
        let (event_tx, event_rx) = unbounded_channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        thread::spawn({
            let event_tx = event_tx.clone();
            let params = params.clone();
            let command_tx = command_tx.clone();
            let serial_number = serial_number.to_owned();
            move || {
                Handler::run(
                    serial_number,
                    params.clone(),
                    open_tx,
                    command_tx,
                    command_rx,
                    event_tx,
                    shutdown_tx,
                )
            }
        });

        open_rx.await.unwrap()?;

        Ok(Ftdi {
            buffer: VecDeque::new(),
            command_tx,
            event_rx,
            error: None,
            shutdown_rx: Some(shutdown_rx),
        })
    }

    pub async fn close(mut self) {
        if self.command_tx.send(Command::Cancel).is_ok() {
            let _ = self.shutdown_rx.take().unwrap().await;
        }
    }

    pub async fn set_params(&mut self, params: SerialParams) -> io::Result<()> {
        let (tx, rx) = oneshot::channel();
        if self
            .command_tx
            .send(Command::SetParams { params, answer: tx })
            .is_err()
        {
            self.error = Some(disconnected_error());
            return Err(disconnected_error());
        }
        match rx.await {
            Ok(x) => {
                if let Err(x) = &x {
                    self.error = Some(clone_io_error(x));
                }
                x
            }
            Err(_) => Err(disconnected_error()),
        }
    }

    fn push_to_output_buffer(&mut self, buf: &mut tokio::io::ReadBuf<'_>) -> bool {
        if self.buffer.is_empty() || buf.remaining() == 0 {
            return false;
        }
        while buf.remaining() != 0 {
            if let Some(x) = self.buffer.pop_front() {
                buf.put_slice(&[x]);
            } else {
                break;
            }
        }
        true
    }

    fn poll_event_queue(&mut self) -> io::Result<()> {
        loop {
            match self.event_rx.try_recv() {
                Ok(Event(Ok(x))) => {
                    self.buffer.extend(x);
                }
                Ok(Event(Err(x))) => {
                    let ret = clone_io_error(&x);
                    self.error = Some(x);
                    return Err(ret);
                }
                Err(TryRecvError::Disconnected) => return Err(disconnected_error()),
                Err(TryRecvError::Empty) => return Ok(()),
            }
        }
    }
}

#[derive(Debug)]
enum Command {
    Cancel,
    PollRead,
    Send(Vec<u8>),
    SetParams {
        params: SerialParams,
        answer: oneshot::Sender<io::Result<()>>,
    },
}

struct Event(io::Result<Vec<u8>>);

struct Handler {
    command_rx: UnboundedReceiver<Command>,
    event_tx: UnboundedSender<Event>,
    _waker: WakerHandle,
    device: FtdiBase,
    close_sender: oneshot::Sender<()>,
}

fn clone_io_error(err: &io::Error) -> io::Error {
    io::Error::new(err.kind(), format!("{}", err))
}

impl Handler {
    fn run(
        serial_number: String,
        params: SerialParams,
        open_channel: oneshot::Sender<io::Result<()>>,
        command_tx: UnboundedSender<Command>,
        command_rx: UnboundedReceiver<Command>,
        event_tx: UnboundedSender<Event>,
        shutdown_tx: oneshot::Sender<()>,
    ) {
        let mut device = match FtdiBase::with_serial_number(&serial_number) {
            Ok(device) => device,
            Err(status) => {
                let _ = open_channel.send(Err(status_to_io_error(status)));
                return;
            }
        };
        if let Err(status) =
            device.set_timeouts(Duration::from_millis(100), Duration::from_millis(100))
        {
            let _ = open_channel.send(Err(status_to_io_error(status)));
            return;
        }

        if let Err(status) = device.set_latency_timer(Duration::from_millis(2)) {
            let _ = open_channel.send(Err(status_to_io_error(status)));
            return;
        }

        if let Err(x) = Self::apply_params(&mut device, &params) {
            let _ = open_channel.send(Err(x));
            return;
        }

        log::debug!("Device configuration succeeded, spawning waker.");
        let waker = match Waker::spawn(&mut device, command_tx.clone()) {
            Ok(x) => x,
            Err(err) => {
                let _ = open_channel.send(Err(err));
                let _ = device.close();
                return;
            }
        };

        let _ = open_channel.send(Ok(()));

        let mut this = Handler {
            command_rx,
            event_tx,
            device,
            _waker: waker,
            close_sender: shutdown_tx,
        };

        if let Err(err) = this.run_loop() {
            let _ = this.event_tx.send(Event(Err(err)));
        }
        let _ = this.device.close();
        let _ = this.close_sender.send(());

        log::debug!("Device handler thread dropped.");
    }

    fn apply_params(device: &mut FtdiBase, params: &SerialParams) -> io::Result<()> {
        device
            .set_baud_rate(params.baud)
            .map_err(status_to_io_error)?;

        device
            .set_data_characteristics(
                params.data_bits.into(),
                params.stop_bits.into(),
                params.parity.into(),
            )
            .map_err(status_to_io_error)?;

        Ok(())
    }

    fn send_data(&mut self, data: Vec<u8>) -> io::Result<()> {
        log::debug!("Sending data... {:?}", data);
        let mut start_idx = 0;
        loop {
            match self.device.write_all(&data[start_idx..]) {
                Ok(_) => break,
                Err(TimeoutError::Timeout { actual, .. }) => {
                    log::debug!("Send timeout occurred. Only sent {} bytes", actual);
                    start_idx += actual;
                }
                Err(TimeoutError::FtStatus(status)) => {
                    log::debug!("Send error: {:?}", status);
                    return Err(status_to_io_error(status));
                }
            }
        }
        log::debug!("send_data() returned.");
        Ok(())
    }

    fn poll_read(&mut self, num_bytes: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0_u8; num_bytes];
        log::debug!("ftdi read_all: {} bytes in queue", num_bytes);
        let ret = match self.device.read_all(&mut buf) {
            Ok(_) => Ok(buf),
            Err(TimeoutError::Timeout { .. }) => {
                // we don't expect this to happen...
                Err(io::Error::new(
                    ErrorKind::TimedOut,
                    "Timeout occurred emptying buffer",
                ))
            }
            Err(TimeoutError::FtStatus(status)) => Err(status_to_io_error(status)),
        };

        log::debug!("read_all() returned");
        ret
    }

    fn run_loop(&mut self) -> io::Result<()> {
        while let Some(msg) = self.command_rx.blocking_recv() {
            match msg {
                Command::Cancel => {
                    log::debug!("Canceling run loop.");
                    break;
                }
                Command::PollRead => {
                    let num_bytes = self.device.queue_status().map_err(status_to_io_error)?;
                    if num_bytes != 0 {
                        let data = self.poll_read(num_bytes)?;
                        if self.event_tx.send(Event(Ok(data))).is_err() {
                            break;
                        }
                    }
                }
                Command::Send(data) => self.send_data(data)?,
                Command::SetParams { params, answer } => {
                    log::debug!("Applying new params: {:?}", params);
                    let result = Self::apply_params(&mut self.device, &params);
                    if let Err(x) = result.as_ref() {
                        log::debug!("Applying params failed: {:?}", x);
                    }
                    let _ = answer.send(result);
                }
            }
        }
        Ok(())
    }
}

impl AsyncRead for Ftdi {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if let Err(x) = self.poll_event_queue() {
            self.error = Some(x);
        }

        if let Some(err) = &self.error {
            return Poll::Ready(Err(clone_io_error(err)));
        }
        if self.push_to_output_buffer(buf) {
            return Poll::Ready(Ok(()));
        }
        loop {
            match self.event_rx.poll_recv(cx) {
                Poll::Ready(Some(Event(Ok(x)))) => {
                    self.buffer.extend(&x);
                    if self.push_to_output_buffer(buf) {
                        return Poll::Ready(Ok(()));
                    }
                }
                Poll::Ready(Some(Event(Err(err)))) => {
                    let ret = clone_io_error(&err);
                    self.error = Some(err);
                    return Poll::Ready(Err(ret));
                }
                Poll::Ready(None) => {
                    return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "Disconnected")));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl AsyncWrite for Ftdi {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        if let Err(x) = self.poll_event_queue() {
            self.error = Some(x);
        }

        if let Some(err) = &self.error {
            return Poll::Ready(Err(clone_io_error(err)));
        }
        if self.command_tx.send(Command::Send(buf.to_vec())).is_err() {
            return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "Disconnected")));
        }
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        // TODO: send a flush message and send a oneshot
        // then keep polling that oneshot
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let _ = self.command_tx.send(Command::Cancel);
        if let Some(shutdown_rx) = self.shutdown_rx.as_mut() {
            match Future::poll(Pin::new(shutdown_rx), cx) {
                Poll::Ready(_) => {
                    self.shutdown_rx.take();
                    Poll::Ready(Ok(()))
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Ready(Ok(()))
        }
    }
}

impl Drop for Ftdi {
    fn drop(&mut self) {
        let _ = self.command_tx.send(Command::Cancel);
    }
}
