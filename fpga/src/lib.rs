use log::{error, info};
use solana_sdk::hash::HASH_BYTES;
use std::io::Error as IoError;
use std::sync::Once;
use warp_devices::{
    varium_c1100::VariumC1100,
    xdma::{Error as XdmaError, XdmaOps},
};

pub use warp_devices::xdma::DmaBuffer;

const VARIUM_HBM_SIZE: u64 = 8 * 1024 * 1024 * 1024;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    CannotCreateDevice(IoError),
    Xdma(XdmaError),
    OutOfHbm,
}

impl From<XdmaError> for Error {
    fn from(e: XdmaError) -> Self {
        Self::Xdma(e)
    }
}

static mut API: Option<FpgaPerf> = None;

struct CardBaseAddrs {
    in_hashes_base: u64,
    num_iters_base: u64,
    out_hashes_base: u64,
}

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

    /// Computes POH hashes for multiple starting hashes.  `hashes` contains input
    /// hashes. `num_iters` contain the numbers of time the SHA-256 hash function should be iterated
    /// over the hash with the same index in `hashes`. The output from the accelerator is returned
    /// in `hashes` and consists of the hashes computed for each input hash.
    pub fn poh_verify_many(&self, hashes: &mut DmaBuffer, num_iters: &DmaBuffer) -> Result<()> {
        let num_hashes = hashes.as_slice().len() / HASH_BYTES;
        // Check that `hashes` and `num_iters` have the same length.
        assert_eq!(num_hashes, num_iters.as_slice().len() / 8);

        let hashes_cap = hashes.get().capacity();
        let num_iters_cap = num_iters.get().capacity();

        if (hashes_cap + num_iters_cap) as u64 > VARIUM_HBM_SIZE {
            return Err(Error::OutOfHbm);
        }

        // TODO: update to `get`
        let base_addrs = self.init_kernel(hashes_cap, num_iters_cap)?;

        // Write the inputs to the card.
        self.device.dma_write(hashes, base_addrs.in_hashes_base)?;
        self.device
            .dma_write(num_iters, base_addrs.num_iters_base)?;

        self.run_kernel()?;

        // Read the results back.
        self.device.dma_read(hashes, base_addrs.out_hashes_base)?;
        Ok(())
    }

    fn init_kernel(
        &self,
        hashes_capacity: usize,
        num_iters_capacity: usize,
    ) -> Result<CardBaseAddrs> {
        let base_addrs = CardBaseAddrs {
            in_hashes_base: 0,
            num_iters_base: hashes_capacity as u64,
            out_hashes_base: (hashes_capacity + num_iters_capacity) as u64,
        };
        Ok(base_addrs)
    }

    fn run_kernel(&self) -> Result<()> {
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
