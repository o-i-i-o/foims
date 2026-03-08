use std::sync::Arc;
use std::sync::Mutex;

// 缓冲区池实现
pub struct BufferPool {
    buffers: Mutex<Vec<Vec<u8>>>,
    max_size: usize,
}

impl BufferPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            buffers: Mutex::new(Vec::with_capacity(max_size)),
            max_size,
        }
    }

    pub fn get(&self) -> Vec<u8> {
        let mut buffers = self.buffers.lock().unwrap();
        buffers.pop().unwrap_or_else(|| Vec::with_capacity(8192))
    }

    pub fn put(&self, mut buffer: Vec<u8>) {
        let mut buffers = self.buffers.lock().unwrap();
        if buffers.len() < self.max_size {
            // 重置缓冲区大小但保留容量
            buffer.clear();
            buffers.push(buffer);
        }
    }
}

// 全局缓冲区池
lazy_static::lazy_static! {
    pub static ref BUFFER_POOL: Arc<BufferPool> = Arc::new(BufferPool::new(100));
}
