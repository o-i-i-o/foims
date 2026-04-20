use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;

// 缓冲区池实现
pub struct BufferPool {
    buffers: Mutex<Vec<Vec<u8>>>,
    max_size: usize,
}

impl BufferPool {
    #[must_use] 
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

static BUFFER_POOL_INNER: OnceLock<Arc<BufferPool>> = OnceLock::new();

pub fn get_buffer_pool() -> Arc<BufferPool> {
    BUFFER_POOL_INNER
        .get_or_init(|| Arc::new(BufferPool::new(100)))
        .clone()
}

#[deprecated(note = "请使用 get_buffer_pool() 函数获取缓冲区池")]
pub static BUFFER_POOL: OnceLock<Arc<BufferPool>> = OnceLock::new();
