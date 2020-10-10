//! 提供向量分配器的简单实现 [`BitmapVectorAllocator`]

use super::VectorAllocator;
use bit_field::BitArray;
use core::cmp::min;

/// Bitmap 中的位数（4K）
// const BITMAP_SIZE: usize = 4096;
const BITMAP_SIZE: usize = 4096;

/// 向量分配器的简单实现，每字节用一位表示
pub struct BitmapVectorAllocator {
    /// 容量，单位为 bitmap 中可以使用的位数，即待分配空间的字节数
    capacity: usize,
    /// 每一位 0 表示空闲
    bitmap: [u8; BITMAP_SIZE / 8],
}

impl VectorAllocator for BitmapVectorAllocator {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: min(BITMAP_SIZE, capacity),
            bitmap: [0u8; BITMAP_SIZE / 8],
        }
    }
    fn alloc(&mut self, size: usize, align: usize) -> Option<usize> {
        // 遍历每一个可能可以分配的字节（对应的bit位）
        for start in (0..self.capacity - size).step_by(align) {
            // 如果这段连续大小为 size 的空间可以分配（bit均为0）则分配
            if (start..start + size).all(|i| !self.bitmap.get_bit(i)) {
                (start..start + size).for_each(|i| self.bitmap.set_bit(i, true));
                return Some(start);
            }
        }
        None
    }
    fn dealloc(&mut self, start: usize, size: usize, _align: usize) {
        // 至少保证要释放空间的起始位置要已经被分配
        // 这里并没有检查保证被释放的空间是之前已经分配的空间
        // 如果使用不当可能发生释放错误空间的问题：分配[1,4],[5,6]；回收[4,6]
        assert!(self.bitmap.get_bit(start));
        (start..start + size).for_each(|i| self.bitmap.set_bit(i, false));
    }
}
