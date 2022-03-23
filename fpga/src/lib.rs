use std::io::Error as IoError;
use warp_devices::varium_c1100::VariumC1100;

/// The length of a round of the packet demultiplexer in the FPGA. The length of a packet batch
/// should be a multiple of this number.
pub const DEMUX_ROUND_LEN: usize = 8;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    CannotCreateDevice(IoError),
}

pub struct FpgaPerf {
    device: VariumC1100,
}

impl FpgaPerf {
    pub fn new() -> Result<Self> {
        let device = VariumC1100::new().map_err(Error::CannotCreateDevice)?;
        Ok(Self { device })
    }
}
