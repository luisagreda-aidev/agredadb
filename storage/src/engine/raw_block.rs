use std::path::Path;
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, FromRawFd};
use io_uring::{opcode, types, IoUring};
use aligned_vec::{AVec, ConstAlign};
use std::sync::Mutex;
use std::io;

/// Motor de Almacenamiento "Nivel Dios" (Kernel Bypass)
/// Con soporte de Fallback dinámico para entornos sin io_uring.
pub struct RawBlockManager {
    fd: i32,
    ring: Option<Mutex<IoUring>>,
}

impl RawBlockManager {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_DIRECT | libc::O_DSYNC) 
            .open(path.as_ref())?;
            
        let fd = file.as_raw_fd();
        std::mem::forget(file);

        let ring = match IoUring::new(128) {
            Ok(r) => Some(Mutex::new(r)),
            Err(_) => None
        };
        
        Ok(Self {
            fd,
            ring,
        })
    }

    pub fn get_fd(&self) -> i32 {
        self.fd
    }

    pub fn read_block(&self, offset: u64, length: usize) -> io::Result<AVec<u8, ConstAlign<4096>>> {
        let mut buffer = AVec::<u8, ConstAlign<4096>>::from_iter(4096, std::iter::repeat(0).take(length));

        if let Some(ref ring_mutex) = self.ring {
            let mut ring = ring_mutex.lock().unwrap();
            let read_e = opcode::Read::new(types::Fd(self.fd), buffer.as_mut_ptr(), length as u32)
                .offset(offset)
                .build();

            unsafe { ring.submission().push(&read_e).expect("SQ Full"); }
            ring.submit_and_wait(1)?;
            let cqe = ring.completion().next().expect("CQE Missing");
            
            if cqe.result() < 0 {
                return Err(io::Error::from_raw_os_error(-cqe.result()));
            }
        } else {
            use std::os::unix::fs::FileExt;
            let file = unsafe { std::fs::File::from_raw_fd(self.fd) };
            file.read_at(&mut buffer, offset)?;
            std::mem::forget(file);
        }
        
        Ok(buffer)
    }

    pub fn write_block(&self, offset: u64, data: &[u8]) -> io::Result<()> {
        if let Some(ref ring_mutex) = self.ring {
            let mut ring = ring_mutex.lock().unwrap();
            let write_e = opcode::Write::new(types::Fd(self.fd), data.as_ptr(), data.len() as u32)
                .offset(offset)
                .build();

            unsafe { ring.submission().push(&write_e).expect("SQ Full"); }
            ring.submit_and_wait(1)?;
            let cqe = ring.completion().next().unwrap();
            if cqe.result() < 0 {
                return Err(io::Error::from_raw_os_error(-cqe.result()));
            }
        } else {
            use std::os::unix::fs::FileExt;
            let file = unsafe { std::fs::File::from_raw_fd(self.fd) };
            file.write_at(data, offset)?;
            std::mem::forget(file);
        }
        Ok(())
    }
}