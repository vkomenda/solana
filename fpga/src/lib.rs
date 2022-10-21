mod poh_core;

use log::{error, info};
use poh_core::{Error as PohCoreError, PohCore, PohCoreBaseAddrs};
use solana_sdk::hash::{Hash, HASH_BYTES};
use std::iter;
use std::sync::Once;
use warp_devices::{
    varium_c1100::VariumC1100,
    xdma::{Error as XdmaError, XdmaOps},
};

pub use warp_devices::xdma::DmaBuffer;

const NUM_POH_CORES: usize = 4;
const VARIUM_URAM_HASHES_BASE: u64 = 0x2_0000_0000;
const VARIUM_URAM_MAX_HASHES_PER_CORE: usize = 32_768;
const VARIUM_URAM_MAX_HASHES: usize = NUM_POH_CORES * VARIUM_URAM_MAX_HASHES_PER_CORE;
const VARIUM_URAM_NUM_ITERS_OFFSET: u64 =
    NUM_POH_CORES as u64 * 32 * VARIUM_URAM_MAX_HASHES_PER_CORE as u64;
const VARIUM_SHELL_BASE: u64 = 0x6_0000;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Xdma(XdmaError),
    InputLengthMismatch,
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
static mut FPGA_DEVICE: Option<VariumC1100> = None;

struct FpgaPerfBuffers {
    dma_buffers: Vec<(DmaBuffer, DmaBuffer)>,
    per_core_sizes: Vec<usize>,
}

impl FpgaPerfBuffers {
    fn new(per_core_sizes: Vec<usize>) -> Self {
        let dma_buffers = per_core_sizes
            .iter()
            .map(|size| (DmaBuffer::new(size * 32), DmaBuffer::new(size * 8)))
            .collect();
        Self {
            dma_buffers,
            per_core_sizes,
        }
    }

    /// Copies the input hashes and numbers of iterations to the input buffers. All arguments should be
    /// correctly initialized before caling this function.
    fn buffer_inputs(&mut self, hashes: &[Hash], num_iters: &[u64]) {
        let mut offset = 0;
        for ((hashes_buf, num_iters_buf), size) in
            self.dma_buffers.iter_mut().zip(&self.per_core_sizes)
        {
            for hash in &hashes[offset..offset + size] {
                hashes_buf.get_mut().extend_from_slice(&hash.to_bytes())
            }
            for num_iters in &num_iters[offset..offset + size] {
                num_iters_buf
                    .get_mut()
                    .extend_from_slice(&num_iters.to_le_bytes())
            }
            offset += size;
        }
    }

    /// Copies the output hashes from the `DmaBuffer` to the output slice. All arguments should be
    /// correctly initialized before caling this function.
    fn unbuffer_outputs(&self, hashes: &mut [Hash]) {
        let mut offset = 0;
        let mut result_buf: [u8; 32] = [0; 32];
        for ((hashes_buf, _), size) in self.dma_buffers.iter().zip(&self.per_core_sizes) {
            for (i, hash) in hashes[offset..offset + size].iter_mut().enumerate() {
                let start_addr = offset + i;
                result_buf
                    .copy_from_slice(&hashes_buf.get()[start_addr * 32..(start_addr + 1) * 32]);
                *hash = Hash::new_from_array(result_buf);
            }
            offset += size;
        }
    }
}

/// FPGA performance interface.
pub struct FpgaPerf {
    cores: Vec<PohCore>,
}

impl FpgaPerf {
    /// Creates a new FPGA performance interface.
    pub fn new(device: &'static VariumC1100) -> Result<Self> {
        let cores: Vec<PohCore> = poh_core_base_addrs()
            .map(|base_addrs| PohCore { device, base_addrs })
            .collect();
        Ok(Self { cores })
    }

    /// Computes PoH hashes for multiple input hashes.  `hashes` contains the input
    /// hashes. `num_iters` contains the numbers of times the SHA-256 hash function should be
    /// iterated over the hash with the same index in `hashes`. The output from the accelerator is
    /// returned in `hashes` and consists of the hashes computed for each input hash.
    pub fn poh_verify_many(&self, hashes: &mut [Hash], num_iters: &[u64]) -> Result<()> {
        let num_hashes = hashes.len() / HASH_BYTES;

        if num_hashes != num_iters.len() {
            return Err(Error::InputLengthMismatch);
        }
        if num_hashes > VARIUM_URAM_MAX_HASHES {
            return Err(Error::OutOfUltraRam);
        }

        let per_core_sizes: Vec<usize> = per_core_sizes(num_hashes).collect();

        for (core, size) in self.cores.iter().zip(&per_core_sizes) {
            core.init(*size as u32)?;
        }

        let mut buffers = FpgaPerfBuffers::new(per_core_sizes);

        buffers.buffer_inputs(hashes, num_iters);

        // Write the inputs to the card.
        for ((hashes_buf, num_iters_buf), core) in buffers.dma_buffers.iter().zip(&self.cores) {
            core.write_hashes(&hashes_buf)?;
            core.write_num_iters(&num_iters_buf)?;
        }

        for core in &self.cores {
            core.start()?;
        }

        for core in &self.cores {
            core.wait_done()?;
        }

        // Read the results back.
        // TODO: asynchronous reads
        for ((hashes_buf, _), core) in buffers.dma_buffers.iter_mut().zip(&self.cores) {
            core.read_hashes(hashes_buf)?;
        }

        // Copy the results from the buffer back to the input slice.
        buffers.unbuffer_outputs(hashes);

        Ok(())
    }
}

/// Computes the base addresses of memories and memory-mapped interfaces of all PoH cores.
fn poh_core_base_addrs() -> impl Iterator<Item = PohCoreBaseAddrs> {
    (0..NUM_POH_CORES as u64).map(|n| PohCoreBaseAddrs {
        shell_base: VARIUM_SHELL_BASE + n * 0x1000,
        uram_hashes_base: VARIUM_URAM_HASHES_BASE + n * 32 * VARIUM_URAM_MAX_HASHES_PER_CORE as u64,
        uram_num_iters_base: VARIUM_URAM_HASHES_BASE
            + VARIUM_URAM_NUM_ITERS_OFFSET
            + n * 8 * VARIUM_URAM_MAX_HASHES_PER_CORE as u64,
    })
}

/// Computes the numbers of hashes to be processed by each core. Core 0 will additionally process
/// the remainder of hashes after the division into `NUM_POH_CORES`.
fn per_core_sizes(num_hashes: usize) -> impl Iterator<Item = usize> {
    let (num_hashes_per_core, rem_hashes_per_core) =
        ((num_hashes / NUM_POH_CORES), (num_hashes % NUM_POH_CORES));
    iter::once(num_hashes_per_core + rem_hashes_per_core)
        .chain(iter::repeat(num_hashes_per_core).take(NUM_POH_CORES - 1))
}

/// Initializes the FPGA performance API.
pub fn init() {
    static INIT_HOOK: Once = Once::new();

    info!("Initializing FPGA API");
    unsafe {
        INIT_HOOK.call_once(|| {
            FPGA_DEVICE = Some(VariumC1100::new().unwrap_or_else(|err| {
                error!("Cannot connect to the FPGA: {:?}", err);
                std::process::exit(1);
            }));
            API = Some(
                FpgaPerf::new(FPGA_DEVICE.as_ref().unwrap()).unwrap_or_else(|err| {
                    error!("Unable to initialize FPGA API: {:?}", err);
                    std::process::exit(1);
                }),
            );
        })
    }
}

/// Returns the FPGA performance API.
pub fn api() -> Option<&'static FpgaPerf> {
    unsafe { API.as_ref() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_addrs_are_correct() {
        let expected_base_addrs = vec![
            PohCoreBaseAddrs {
                shell_base: 0x6_0000,
                uram_hashes_base: 0x2_0000_0000,
                uram_num_iters_base: 0x2_0040_0000,
            },
            PohCoreBaseAddrs {
                shell_base: 0x6_1000,
                uram_hashes_base: 0x2_0010_0000,
                uram_num_iters_base: 0x2_0044_0000,
            },
            PohCoreBaseAddrs {
                shell_base: 0x6_2000,
                uram_hashes_base: 0x2_0020_0000,
                uram_num_iters_base: 0x2_0048_0000,
            },
            PohCoreBaseAddrs {
                shell_base: 0x6_3000,
                uram_hashes_base: 0x2_0030_0000,
                uram_num_iters_base: 0x2_004C_0000,
            },
        ];
        assert_eq!(
            poh_core_base_addrs().collect::<Vec<_>>(),
            expected_base_addrs
        );
    }

    #[test]
    fn per_core_sizes_are_correct() {
        assert_eq!(per_core_sizes(0).collect::<Vec<_>>(), vec![0, 0, 0, 0]);
        assert_eq!(per_core_sizes(1).collect::<Vec<_>>(), vec![1, 0, 0, 0]);
        assert_eq!(per_core_sizes(10).collect::<Vec<_>>(), vec![4, 2, 2, 2]);
        assert_eq!(
            per_core_sizes(201).collect::<Vec<_>>(),
            vec![51, 50, 50, 50]
        );
        assert_eq!(
            per_core_sizes(100_003).collect::<Vec<_>>(),
            vec![25_003, 25_000, 25_000, 25_000]
        );
    }

    #[test]
    fn buffer_unbuffer() {
        let per_core_sizes = vec![2, 1, 1, 1];
        let expected_outputs: Vec<_> = iter::repeat_with(Hash::new_unique).take(5).collect();
        let input_hashes = expected_outputs.clone();
        let mut output_hashes: Vec<_> = iter::repeat_with(Hash::new_unique).take(5).collect();
        let num_iters: Vec<_> = iter::repeat(0).take(5).collect();
        let mut buffers = FpgaPerfBuffers::new(per_core_sizes);

        buffers.buffer_inputs(&input_hashes, &num_iters);
        println!("{:?}", buffers.dma_buffers);
        buffers.unbuffer_outputs(&mut output_hashes);

        assert_eq!(output_hashes, expected_outputs);
    }
}
