use std::io;

use async_ftdi::{DataBits, Ftdi, Parity, SerialParams, StopBits};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        let mut read_buf = [0_u8; 4];
        ftdi.read_exact(&mut read_buf).await.unwrap();
        for x in read_buf {
            print!("0x{:x} ", x);
        }
        print!("\n");
    }

    Ok(())
}
