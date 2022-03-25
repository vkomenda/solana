use log::{error, info};
use solana_sdk::hash::HASH_BYTES;
use std::io::Error as IoError;
use std::sync::Once;
use warp_devices::{
    varium_c1100::VariumC1100,
    xdma::{Error as XdmaError, XdmaOps},
};

pub use warp_devices::xdma::DmaBuffer;

/// The length of a round of the packet demultiplexer in the FPGA. The length of a packet batch
/// should be a multiple of this number.
pub const DEMUX_ROUND_LEN: usize = 8;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    CannotCreateDevice(IoError),
    Xdma(XdmaError),
}

impl From<XdmaError> for Error {
    fn from(e: XdmaError) -> Self {
        Self::Xdma(e)
    }
}

static mut API: Option<FpgaPerf> = None;

/// FPGA performance interface.
pub struct FpgaPerf {
    device: VariumC1100,
}

impl FpgaPerf {
    /// Create a new FPGA interface.
    pub fn new() -> Result<Self> {
        let device = VariumC1100::new().map_err(Error::CannotCreateDevice)?;
        Ok(Self { device })
    }

    /// Computes POH hashes for multiple starting hashes.  `buf` contains packets of the kind
    /// `(hash, n)` where the `n` is the number of times the SHA-256 hash function should be
    /// iterated over `hash`. `buf` should consist of a number of batches of 8 packets each for the
    /// purpose of alignment with the length of a demultiplexer round. The output from the
    /// accelerator is returned in `buf` and consists of the hashes computed for each input packet.
    pub fn poh_verify_many(&self, buf: &mut DmaBuffer) -> Result<()> {
        let num_packets = buf.as_slice().len() / (HASH_BYTES + 8);
        // Check the packet batch alignment with the FPGA demultiplexer.
        assert_eq!(
            buf.as_slice().len() % ((HASH_BYTES + 8) * DEMUX_ROUND_LEN),
            0
        );
        self.device.dma_write(buf, 0)?;
        buf.get_mut().truncate(num_packets * HASH_BYTES);
        self.device.dma_read(buf, 0)?;
        Ok(())
    }
}

pub fn init() {
    static INIT_HOOK: Once = Once::new();

    info!("Initializing FPGA API");
    unsafe {
        INIT_HOOK.call_once(|| {
            API = Some(FpgaPerf::new().unwrap_or_else(|err| {
                error!("Unable to initialize FPGA API: {:?}", err);
                std::process::exit(1);
            }));
        })
    }
}

pub fn api() -> Option<&'static FpgaPerf> {
    unsafe { API.as_ref() }
}
