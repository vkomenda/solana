mod poh_core;

use log::{error, info};
use poh_core::{DataBaseAddrs, Error as PohCoreError, PohCoreOps, PohCoreParam};
use solana_sdk::hash::HASH_BYTES;
use std::io::Error as IoError;
use std::sync::Once;
use warp_devices::{
    varium_c1100::VariumC1100,
    xdma::{Error as XdmaError, XdmaOps},
};

pub use warp_devices::xdma::DmaBuffer;

const VARIUM_HBM_SIZE: u64 = 8 * 1024 * 1024 * 1024;
const VARIUM_HBM_BASE: u64 = 0;
/// UltraRAM hashes section of 8 MB, enough for 262,144 hashes.
const VARIUM_URAM_HASHES_BASE: u64 = 0x2_0000_0000;
/// UltraRAM num_iters section of matching size.
const VARIUM_URAM_NUM_ITERS_BASE: u64 = 0x2_0080_0000;
const VARIUM_URAM_MAX_HASHES: usize = 262_144;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    CannotCreateDevice(IoError),
    Xdma(XdmaError),
    OutOfHbm,
    OutOfUltraRam,
    PohCore(PohCoreError),
}

impl From<XdmaError> for Error {
    fn from(e: XdmaError) -> Self {
        Self::Xdma(e)
    }
}

impl From<PohCoreError> for Error {
    fn from(e: PohCoreError) -> Self {
        Self::PohCore(e)
    }
}

static mut API: Option<FpgaPerf> = None;

impl PohCoreParam for VariumC1100 {
    const BASE_ADDR: u64 = 0x0005_0000;
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

        let base_addrs = if num_hashes <= VARIUM_URAM_MAX_HASHES {
            if hashes_cap / HASH_BYTES > VARIUM_URAM_MAX_HASHES
                || num_iters_cap / 8 > VARIUM_URAM_MAX_HASHES
            {
                return Err(Error::OutOfHbm);
            }
            // Use UltraRAM
            DataBaseAddrs {
                in_hashes_base: VARIUM_URAM_HASHES_BASE,
                num_iters_base: VARIUM_URAM_NUM_ITERS_BASE,
                out_hashes_base: VARIUM_URAM_HASHES_BASE,
            }
        } else {
            // Use HBM
            if (2 * hashes_cap + num_iters_cap) as u64 > VARIUM_HBM_SIZE {
                return Err(Error::OutOfHbm);
            }
            DataBaseAddrs {
                in_hashes_base: VARIUM_HBM_BASE,
                num_iters_base: hashes_cap as u64,
                out_hashes_base: (hashes_cap + num_iters_cap) as u64,
            }
        };
        self.device.init_poh(base_addrs, num_hashes as u32)?;

        // Write the inputs to the card.
        self.device.dma_write(hashes, base_addrs.in_hashes_base)?;
        self.device
            .dma_write(num_iters, base_addrs.num_iters_base)?;

        self.device.run_poh()?;

        // Read the results back.
        self.device.dma_read(hashes, base_addrs.out_hashes_base)?;
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
