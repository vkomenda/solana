use enum_iterator::Sequence;
use warp_devices::{
    varium_c1100::VariumC1100,
    xdma::{DmaBuffer, Error as XdmaError, XdmaOps},
};

#[derive(Copy, Clone, Debug, Sequence, PartialEq)]
#[repr(u32)]
enum ControlRegBit {
    Start = 0b0001, // (Read/Write/COH)
    Done = 0b0010,  // (Read/COR)
    Idle = 0b0100,  // (Read)
                    // Ready = 0b1000,            // (Read)
                    // AutoRestart = 0x1000_0000, // (Read/Write)
}

#[derive(Copy, Clone, Debug, Sequence, PartialEq)]
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

pub type Result<T> = std::result::Result<T, Error>;
pub type XdmaResult<T> = std::result::Result<T, XdmaError>;

#[derive(Debug)]
pub enum Error {
    XdmaFailed(XdmaError),
}

impl From<XdmaError> for Error {
    fn from(e: XdmaError) -> Self {
        Self::XdmaFailed(e)
    }
}

/// Base addresses of memories and memory-mapped register interfaces of a PoH core.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PohCoreBaseAddrs {
    pub shell_base: u64,
    pub hashes_base: u64,
    pub num_iters_base: u64,
}

/// PoH (Vitis HLS) core.
pub struct PohCore {
    pub device: &'static VariumC1100,
    pub base_addrs: PohCoreBaseAddrs,
}

impl XdmaOps for PohCore {
    fn shell_read(&self, buf: &mut [u8], offset: u64) -> XdmaResult<()> {
        let addr = self.base_addrs.shell_base + offset;
        self.device.shell_read(buf, addr)?;
        // println!("@0x{:x} = {:02x?}", addr, buf);
        Ok(())
    }

    fn shell_write(&self, buf: &[u8], offset: u64) -> XdmaResult<()> {
        self.device
            .shell_write(buf, self.base_addrs.shell_base + offset)
    }

    fn dma_read(&self, buf: &mut DmaBuffer, offset: u64) -> XdmaResult<()> {
        self.device.dma_read(buf, offset)
    }

    fn dma_write(&self, buf: &DmaBuffer, offset: u64) -> XdmaResult<()> {
        self.device.dma_write(buf, offset)
    }
}

impl PohCore {
    pub fn init(&self, num_hashes: u32) -> Result<()> {
        // println!("init");
        let mut control_reg = 0;
        let mut control_bytes = [0u8; 4];

        // Wait for IDLE.
        while control_reg & ControlRegBit::Idle as u32 != ControlRegBit::Idle as u32 {
            // for n_reg in 0..16 {
            // self.shell_read(&mut control_bytes, n_reg << 2)?;
            self.shell_read(&mut control_bytes, 0)?;
            control_reg = u32::from_le_bytes(control_bytes);
        }

        // // Write the inputs.
        let in_hashes_bytes = self.base_addrs.hashes_base.to_le_bytes();
        self.shell_write(&in_hashes_bytes[0..4], PohCoreReg::InHashesLow as u64)?;
        self.shell_write(&in_hashes_bytes[4..8], PohCoreReg::InHashesHigh as u64)?;
        let num_iters_bytes = self.base_addrs.num_iters_base.to_le_bytes();
        self.shell_write(&num_iters_bytes[0..4], PohCoreReg::NumItersLow as u64)?;
        self.shell_write(&num_iters_bytes[4..8], PohCoreReg::NumItersHigh as u64)?;
        let num_hashes_bytes = num_hashes.to_le_bytes();
        self.shell_write(&num_hashes_bytes, PohCoreReg::NumHashes as u64)?;
        self.shell_write(&in_hashes_bytes[0..4], PohCoreReg::OutHashesLow as u64)?;
        self.shell_write(&in_hashes_bytes[4..8], PohCoreReg::OutHashesHigh as u64)?;

        Ok(())
    }

    pub fn start(&self) -> Result<()> {
        // Send the start command.
        let start_cmd = (ControlRegBit::Start as u32).to_le_bytes();
        self.shell_write(&start_cmd, 0)?;

        Ok(())
    }

    pub fn wait_done(&self) -> Result<()> {
        let mut control_reg = 0;
        let mut control_bytes = [0u8; 4];

        while control_reg & ControlRegBit::Done as u32 != ControlRegBit::Done as u32 {
            self.shell_read(&mut control_bytes, 0)?;
            control_reg = u32::from_le_bytes(control_bytes);
        }

        Ok(())
    }

    pub fn write_hashes(&self, hashes_buf: &DmaBuffer) -> Result<()> {
        self.device
            .dma_write(hashes_buf, self.base_addrs.hashes_base)?;
        Ok(())
    }

    pub fn write_num_iters(&self, num_iters_buf: &DmaBuffer) -> Result<()> {
        self.device
            .dma_write(num_iters_buf, self.base_addrs.num_iters_base)?;
        Ok(())
    }

    pub fn read_hashes(&self, hashes_buf: &mut DmaBuffer) -> Result<()> {
        self.device
            .dma_read(hashes_buf, self.base_addrs.hashes_base)?;
        Ok(())
    }
}
