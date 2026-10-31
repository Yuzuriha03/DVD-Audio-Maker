// FFmpeg Object Pool - Reduces allocation overhead
// Reuses Frame and Packet objects instead of allocating new ones

use crate::ffmpeg::{
    Frame, Packet, av_frame_alloc, av_frame_free, av_frame_unref, av_packet_alloc, av_packet_free,
    av_packet_unref,
};
use std::sync::{LazyLock, Mutex};

/// Frame pool for reusing FFmpeg frame objects
pub struct FramePool {
    available: Mutex<Vec<usize>>,
    capacity: usize,
    allocated: Mutex<usize>,
}

impl FramePool {
    pub fn new(capacity: usize) -> Self {
        Self {
            available: Mutex::new(Vec::with_capacity(capacity)),
            capacity,
            allocated: Mutex::new(0),
        }
    }

    /// Acquire a frame from the pool
    pub fn acquire(&self) -> *mut Frame {
        let mut available = self.available.lock().unwrap();

        if let Some(frame) = available.pop() {
            frame as *mut Frame
        } else {
            // Allocate new if pool is empty and under capacity
            let mut allocated = self.allocated.lock().unwrap();
            if *allocated < self.capacity {
                *allocated += 1;
                unsafe { av_frame_alloc() }
            } else {
                // Pool exhausted, allocate temporary
                unsafe { av_frame_alloc() }
            }
        }
    }

    /// Return a frame acquired from this pool.
    fn release(&self, frame: *mut Frame) {
        if frame.is_null() {
            return;
        }

        // Unref to clear data
        unsafe { av_frame_unref(frame) };

        let mut available = self.available.lock().unwrap();
        if available.len() < self.capacity {
            available.push(frame as usize);
        } else {
            // Pool full, free the frame
            let mut frame = frame;
            unsafe { av_frame_free(&mut frame) };
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> (usize, usize) {
        let available = self.available.lock().unwrap().len();
        let allocated = *self.allocated.lock().unwrap();
        (available, allocated)
    }
}

impl Drop for FramePool {
    fn drop(&mut self) {
        let mut available = self.available.lock().unwrap();
        for frame in available.drain(..) {
            let mut frame = frame as *mut Frame;
            unsafe { av_frame_free(&mut frame) };
        }
    }
}

/// Packet pool for reusing FFmpeg packet objects
pub struct PacketPool {
    available: Mutex<Vec<usize>>,
    capacity: usize,
    allocated: Mutex<usize>,
}

impl PacketPool {
    pub fn new(capacity: usize) -> Self {
        Self {
            available: Mutex::new(Vec::with_capacity(capacity)),
            capacity,
            allocated: Mutex::new(0),
        }
    }

    /// Acquire a packet from the pool
    pub fn acquire(&self) -> *mut Packet {
        let mut available = self.available.lock().unwrap();

        if let Some(packet) = available.pop() {
            packet as *mut Packet
        } else {
            let mut allocated = self.allocated.lock().unwrap();
            if *allocated < self.capacity {
                *allocated += 1;
                unsafe { av_packet_alloc() }
            } else {
                unsafe { av_packet_alloc() }
            }
        }
    }

    /// Return a packet to the pool.
    ///
    /// Return a packet acquired from this pool.
    fn release(&self, packet: *mut Packet) {
        if packet.is_null() {
            return;
        }

        unsafe { av_packet_unref(packet) };

        let mut available = self.available.lock().unwrap();
        if available.len() < self.capacity {
            available.push(packet as usize);
        } else {
            let mut packet = packet;
            unsafe { av_packet_free(&mut packet) };
        }
    }

    pub fn stats(&self) -> (usize, usize) {
        let available = self.available.lock().unwrap().len();
        let allocated = *self.allocated.lock().unwrap();
        (available, allocated)
    }
}

impl Drop for PacketPool {
    fn drop(&mut self) {
        let mut available = self.available.lock().unwrap();
        for packet in available.drain(..) {
            let mut packet = packet as *mut Packet;
            unsafe { av_packet_free(&mut packet) };
        }
    }
}

/// Global frame pool - 16 frames should be enough for most use cases
pub static FRAME_POOL: LazyLock<FramePool> = LazyLock::new(|| FramePool::new(16));

/// Global packet pool
pub static PACKET_POOL: LazyLock<PacketPool> = LazyLock::new(|| PacketPool::new(32));

/// RAII guard for automatic frame return to pool
pub struct PooledFrame(*mut Frame);

impl PooledFrame {
    pub fn new() -> Self {
        Self(FRAME_POOL.acquire())
    }

    pub fn as_ptr(&self) -> *mut Frame {
        self.0
    }
}

impl Default for PooledFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PooledFrame {
    fn drop(&mut self) {
        FRAME_POOL.release(self.0);
    }
}

/// RAII guard for automatic packet return to pool
pub struct PooledPacket(*mut Packet);

impl PooledPacket {
    pub fn new() -> Self {
        Self(PACKET_POOL.acquire())
    }

    pub fn as_ptr(&self) -> *mut Packet {
        self.0
    }
}

impl Default for PooledPacket {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PooledPacket {
    fn drop(&mut self) {
        PACKET_POOL.release(self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_pool() {
        let pool = FramePool::new(4);

        let f1 = pool.acquire();
        let f2 = pool.acquire();

        assert!(!f1.is_null());
        assert!(!f2.is_null());

        pool.release(f1);
        pool.release(f2);

        let (available, allocated) = pool.stats();
        assert_eq!(available, 2);
        assert_eq!(allocated, 2);
    }

    #[test]
    fn test_pooled_frame_raii() {
        {
            let _frame = PooledFrame::new();
            // Frame automatically returned to pool when dropped
        }

        let (available, _) = FRAME_POOL.stats();
        assert!(available > 0);
    }
}
