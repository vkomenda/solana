use enum_iterator_derive::IntoEnumIterator;
use warp_devices::xdma::{Error as XdmaError, XdmaOps};

#[repr(u32)]
enum ControlRegBit {
    Start = 0b0001, // (Read/Write/COH)
    Done = 0b0010,  // (Read/COR)
    Idle = 0b0100,  // (Read)
                    // Ready = 0b1000,            // (Read)
                    // AutoRestart = 0x1000_0000, // (Read/Write)
}

#[derive(Copy, Clone, Debug, IntoEnumIterator, PartialEq)]
#[repr(u64)]
enum PohCoreReg {
    Control = 0,
    GlobalInterruptEnable = 0x04,
    IpInterruptEnable = 0x08,
    IpInterruptStatus = 0x0c,
    InHashesLow = 0x10,
    InHashesHigh = 0x14,
    NumItersLow = 0x1c,
    NumItersHigh = 0x20,
    NumHashes = 0x28,
    OutHashesLow = 0x30,
    OutHashesHigh = 0x34,
}

/// POH (Vitis HLS) core parameters
pub trait PohCoreParam {
    const BASE_ADDR: u64;
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    XdmaFailed(XdmaError),
}

impl From<XdmaError> for Error {
    fn from(e: XdmaError) -> Self {
        Self::XdmaFailed(e)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct DataBaseAddrs {
    pub in_hashes_base: u64,
    pub num_iters_base: u64,
    pub out_hashes_base: u64,
}

pub trait PohCoreOps {
    /// Initialises the POH core.
    fn init_poh(&self, base_addrs: DataBaseAddrs, num_hashes: u32) -> Result<()>;
    /// Starts computing the hashes and waits until the POH core reaches the DONE state.
    fn run_poh(&self) -> Result<()>;
}

impl<T> PohCoreOps for T
where
    T: XdmaOps + PohCoreParam,
{
    fn init_poh(&self, base_addrs: DataBaseAddrs, num_hashes: u32) -> Result<()> {
        let mut control_reg = 0;
        let mut control_bytes = [0u8; 4];

        // Wait for IDLE.
        while control_reg & ControlRegBit::Idle as u32 != ControlRegBit::Idle as u32 {
            self.shell_read(&mut control_bytes, T::BASE_ADDR)?;
            control_reg = u32::from_le_bytes(control_bytes);
        }

        // Write the inputs.
        let in_hashes_bytes = base_addrs.in_hashes_base.to_le_bytes();
        self.shell_write(
            &in_hashes_bytes[0..4],
            T::BASE_ADDR + PohCoreReg::InHashesLow as u64,
        )?;
        self.shell_write(
            &in_hashes_bytes[4..8],
            T::BASE_ADDR + PohCoreReg::InHashesHigh as u64,
        )?;
        let num_iters_bytes = base_addrs.num_iters_base.to_le_bytes();
        self.shell_write(
            &num_iters_bytes[0..4],
            T::BASE_ADDR + PohCoreReg::NumItersLow as u64,
        )?;
        self.shell_write(
            &num_iters_bytes[4..8],
            T::BASE_ADDR + PohCoreReg::NumItersHigh as u64,
        )?;
        let num_hashes_bytes = num_hashes.to_le_bytes();
        self.shell_write(
            &num_hashes_bytes,
            T::BASE_ADDR + PohCoreReg::NumHashes as u64,
        )?;
        let out_hashes_bytes = base_addrs.out_hashes_base.to_le_bytes();
        self.shell_write(
            &out_hashes_bytes[0..4],
            T::BASE_ADDR + PohCoreReg::OutHashesLow as u64,
        )?;
        self.shell_write(
            &out_hashes_bytes[4..8],
            T::BASE_ADDR + PohCoreReg::OutHashesHigh as u64,
        )?;

        Ok(())
    }

    fn run_poh(&self) -> Result<()> {
        let mut control_reg = 0;
        let mut control_bytes = [0u8; 4];

        // Send the start command.
        let start_cmd = (ControlRegBit::Start as u32).to_le_bytes();
        self.shell_write(&start_cmd, T::BASE_ADDR)?;

        // Wait until DONE.
        while control_reg & ControlRegBit::Done as u32 != ControlRegBit::Done as u32 {
            self.shell_read(&mut control_bytes, T::BASE_ADDR)?;
            control_reg = u32::from_le_bytes(control_bytes);
        }

        Ok(())
    }
}
