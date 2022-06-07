use std::{io, time::Duration};

use async_ftdi::{DataBits, Ftdi, Parity, SerialParams, StopBits};
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> io::Result<()> {
    let ftdis = Ftdi::list_all().await?;
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
    let mut ftdi = Ftdi::open(&ftdi_info.serial_number, &params).await?;

    for _ in 0..20 {
        ftdi.write_all(&[0xAB, 0xCD, 0xEF, 0x12, 0x23, 0x45])
            .await
            .unwrap();
        sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}
