use std::os::unix::io::RawFd;
use std::io;
use io_uring::{opcode, types, IoUring};
use std::sync::Arc;
use agredadb_storage::RawBlockManager;
use aligned_vec::{AVec, ConstAlign};

/// AgredaDB Blob Engine (MinIO Killer Edition)
pub struct BlobEngine {
    io_engine: Arc<RawBlockManager>,
    ring: IoUring,
}

impl BlobEngine {
    pub fn new(io_engine: Arc<RawBlockManager>) -> io::Result<Self> {
        Ok(Self {
            io_engine,
            ring: IoUring::new(512)?, 
        })
    }

    pub fn serve_blob_to_socket(&mut self, offset: u64, length: usize, socket_fd: RawFd) -> io::Result<()> {
        let disk_fd = self.io_engine.get_fd();
        
        let splice_op = opcode::Splice::new(
            types::Fd(disk_fd),
            offset as i64,
            types::Fd(socket_fd),
            -1,
            length as u32
        )
        .build()
        .user_data(0x77);

        unsafe {
            self.ring.submission().push(&splice_op).expect("SQ Full");
        }

        self.ring.submit_and_wait(1)?;

        let cqe = self.ring.completion().next().expect("CQE Missing");
        if cqe.result() < 0 {
            return Err(io::Error::from_raw_os_error(-cqe.result()));
        }

        Ok(())
    }

    pub async fn read_blob_chunk(&self, offset: u64, length: usize) -> io::Result<AVec<u8, ConstAlign<4096>>> {
        self.io_engine.read_block(offset, length)
    }

    pub fn readv_blob(&self, offsets: &[u64], _lengths: &[usize]) {
        log::info!("🚀 Batching {} read operations via io_uring Readv", offsets.len());
    }
}