//! ヒープアロケータの初期化
//!
//! `linked_list_allocator`クレートに実際のメモリ管理アルゴリズムを任せ、
//! カーネル側は「どの仮想アドレス範囲をヒープとして使うか」を決めて
//! ページをマッピングするところまでを担当する。これが整うと、
//! `alloc`クレート経由で`Box`・`Vec`・`String`などが使えるようになる。

use linked_list_allocator::LockedHeap;
use x86_64::{
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

/// ヒープの開始アドレス。他の用途と衝突しない、適当な高位アドレスを選ぶ。
pub const HEAP_START: usize = 0x_4444_4444_0000;
/// ヒープのサイズ。まずは100KiBという控えめな値から始める。
pub const HEAP_SIZE: usize = 100 * 1024;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// ヒープ用の仮想アドレス範囲を実際にページマッピングし、アロケータを
/// 初期化する。カーネル起動時に一度だけ呼ぶ。
pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush();
        }
    }

    unsafe {
        ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE);
    }

    Ok(())
}