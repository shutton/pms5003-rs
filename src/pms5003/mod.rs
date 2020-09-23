use actix::prelude::*;
use anyhow::{anyhow, Result};
use bincode::{DefaultOptions, Options};
use log::*;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::mem;
use std::thread;
use std::time::Duration;

mod actor;
pub use actor::{GetLastFrame, Monitor};

const START_OF_FRAME: &[u8] = &[0x42, 0x4d]; // "BM"

/**
 * A data frame (excluding the marker and frame size)
 */
#[derive(Serialize, Deserialize, Debug, Default, MessageResponse, Clone)]
pub struct Frame {
    pm10_standard: u16,
    pm25_standard: u16,
    pm100_standard: u16,
    pm10_env: u16,
    pm25_env: u16,
    pm100_env: u16,
    particles_03um: u16,
    particles_05um: u16,
    particles_10um: u16,
    particles_25um: u16,
    particles_50um: u16,
    particles_100um: u16,
    unused: u16,
    checksum: u16,
}

/**
 * Standard serial communication settings for the PMS5003
 */
const SERIAL_SETTINGS: serialport::SerialPortSettings = serialport::SerialPortSettings {
    baud_rate: 9600,
    data_bits: serialport::DataBits::Eight,
    flow_control: serialport::FlowControl::None,
    parity: serialport::Parity::None,
    stop_bits: serialport::StopBits::One,
    timeout: std::time::Duration::from_millis(1000),
};

pub async fn monitor<S>(helper: Addr<actor::Monitor>, device: S)
where
    S: Into<String>,
{
    let wait = Duration::from_millis(250);

    let device = device.into();
    tokio::spawn(async move {
        loop {
            let mut port =
                // serialport::open_with_settings("/dev/ttyAMA0", &SERIAL_SETTINGS).unwrap();
                serialport::open_with_settings(&device, &SERIAL_SETTINGS).unwrap();
            info!("Reading frames");
            while let Ok(frame) = get_frame(&mut port) {
                if let Err(e) = helper.send(actor::NewFrame(frame)).await {
                    warn!("Couldn't send frame to helper: {}", e);
                }
            }
            // This would be a good place to reset the sensor
            info!("sleeping for {:?} before roepening port", wait);
            tokio::time::delay_for(wait).await;
        }
    });
    info!("tokio spawn done");
}

fn get_frame(port: &mut Box<dyn serialport::SerialPort>) -> Result<Frame> {
    // Read until the start-of-frame signature is seen
    let mut sof_index: usize = 0;
    loop {
        // let n_ready = port.bytes_to_read()? as usize;
        let mut sof_buf = [0u8; 1];
        port.read(&mut sof_buf)?;
        if sof_buf[0] == START_OF_FRAME[sof_index] {
            sof_index += 1;
            if sof_index == START_OF_FRAME.len() {
                // Found it
                break;
            }
        } else {
            // Start over
            sof_index = 0;
        }
    }
    // Read the frame start
    while port.bytes_to_read()? < 2 {
        thread::sleep(Duration::from_millis(10));
    }

    let mut frame_size_buf = [0u8; mem::size_of::<u16>()];
    port.read(&mut frame_size_buf)?;

    let frame_size = u16::from_be_bytes(frame_size_buf.try_into().unwrap());
    if frame_size as usize != mem::size_of::<Frame>() {
        // Bogus -- bail and look for a new signature
        return Err(anyhow!(
            "Invalid frame size of {} (expect {})",
            frame_size,
            mem::size_of::<Frame>()
        ));
    }

    // Wait for the entire frame to be ready
    while port.bytes_to_read()? < frame_size as u32 {
        thread::sleep(Duration::from_millis(10));
    }

    let mut frame_bytes = [0; std::mem::size_of::<Frame>()];
    port.read(&mut frame_bytes)?;
    let de = DefaultOptions::new()
        .with_big_endian()
        .with_fixint_encoding();
    let frame: Frame = de.deserialize(&frame_bytes)?;

    Ok(frame)
}
