use libc::{
    MAP_FAILED,
    MAP_SHARED,
    O_RDWR,
    PROT_READ,
    PROT_WRITE,
    c_int,
    c_void,
    close,
    mmap,
    open,
};

use std::ffi::CString;
use std::ptr;

use err_str;
use util::read_file;

/*
 * P2SB private registers.
 */
const P2SB_PORTID_SHIFT: u32 = 16;

/*
 * GPIO sideband registers.
 */
const REG_PCH_GPIO_PADBAR: u32 = 0xc;

pub struct Sideband {
    pub addr: u64,
}

impl Sideband {
    pub unsafe fn new(sbreg_phys: usize) -> Result<Sideband, String> {
        let mem_str = CString::new("/dev/mem").unwrap();
        let memfd: c_int = open(mem_str.as_ptr(), O_RDWR);
        if memfd == -1 {
            return Err(format!("Failed to open /dev/mem"));
        }

        let sbreg_virt = mmap(
            sbreg_phys as *mut c_void,
            1<<24,
            PROT_READ | PROT_WRITE,
            MAP_SHARED,
            memfd,
            sbreg_phys as i64
        );

        close(memfd);

        if sbreg_virt == MAP_FAILED {
            return Err(format!("Failed to map Sideband memory"));
        }

        Ok(Sideband {
            addr: sbreg_virt as u64
        })
    }

    pub unsafe fn read(&self, port: u8, reg: u32) -> u32 {
        let offset = ((port as u64) << P2SB_PORTID_SHIFT) + reg as u64;
        if offset < 1<<24 {
            let addr = self.addr + offset;
            ptr::read(addr as *mut u32)
        } else {
            0
        }
    }

    pub unsafe fn gpio(&self, port: u8, pad: u8) -> u64 {
        let padbar: u32 = self.read(port, REG_PCH_GPIO_PADBAR);

        let dw0: u32 = self.read(port, padbar + pad as u32 * 8);
        let dw1: u32 = self.read(port, padbar + pad as u32 * 8 + 4);

        dw0 as u64 | (dw1 as u64) << 32
    }
}

pub struct HotPlugDetect {
    sideband: Sideband,
    port: u8,
    pins: [u8; 3]
}

impl HotPlugDetect {
    pub unsafe fn new() -> Result<HotPlugDetect, String> {
        match read_file("/sys/class/dmi/id/product_version").map_err(err_str)?.trim() {
            "oryp4" | "oryp4-b" => Ok(HotPlugDetect {
                sideband: Sideband::new(0xFD000000)?,
                port: 0x6A,
                pins: [40, 42, 44],
            }),
            other => Err(format!("{} does not support hotplug detection", other))
        }
    }

    pub unsafe fn detect(&self) -> [bool; 3] {
        let mut hpd = [false; 3];
        for i in 0..self.pins.len() {
            let data = self.sideband.gpio(self.port, self.pins[i]);
            hpd[i] = data & 2 == 2;
        }
        hpd
    }
}
