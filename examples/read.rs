use std::io;

use async_ftdi::{DataBits, Ftdi, Parity, SerialParams, StopBits};

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
    let ftdi = Ftdi::open(&ftdi_info.serial_number, &params).await?;
    todo!();
}
