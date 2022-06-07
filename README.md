# async-ftdi

Library on top of [`libftd2xx`](https://github.com/ftdi-rs/libftd2xx/) implementing asynchronous IO.

This crate exposes an `FTDI` struct which implements both `AsyncRead` as well `AsyncWrite`.

## OS Support

This crate has only been tested with the following targets:

 * `x86_64-pc-windows-msvc`
 * `x86_64-unknown-linux-gnu`

## Reader Example


```rust
use std::{io, time::Duration};

use async_ftdi::{DataBits, Ftdi, Parity, SerialParams, StopBits};
use tokio::io::AsyncReadExt;
use tokio::{sync::oneshot, task, time::sleep};

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();

    let ftdis = Ftdi::list_devices().await?;
    if ftdis.len() == 0 {
        println!("Not FTDIs connected!");
        return Ok(());
    }
    let ftdi_info = ftdis.first().unwrap();
    println!("FTDI found, opening device: {}", &ftdi_info.serial_number);
    let params = SerialParams {
        baud: 115200,
        data_bits: DataBits::Eight,
        stop_bits: StopBits::One,
        parity: Parity::Even,
    };
    let ftdi = Ftdi::open(&ftdi_info.serial_number, &params).await?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    task::spawn(reader_and_cancel(ftdi, shutdown_rx));
    sleep(Duration::from_secs(2)).await;
    let _ = shutdown_tx.send(());
    Ok(())
}

async fn reader_and_cancel(ftdi: Ftdi, shutdown: oneshot::Receiver<()>) {
    tokio::select! {
        _ = reader(ftdi) => {},
        _ = shutdown => {}
    }
}

async fn reader(mut ftdi: Ftdi) -> io::Result<()> {
    let mut print_cnt = 0;
    loop {
        let x = ftdi.read_u8().await?;
        print!("0x{:x} ", x);
        print_cnt += 1;
        if print_cnt == 8 {
            print!("\n");
            print_cnt = 0;
        }
    }
}
```