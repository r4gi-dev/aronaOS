//! メモリ管理: ページング(仮想アドレス→物理アドレス変換)とフレーム割り当て
//!
//! ブートローダーが用意した物理メモリマップを使い、実際に使用可能な
//! メモリ領域(フレーム)を管理する。ヒープアロケータ(allocator.rs)が
//! 実際にメモリを確保する際、ここで用意した仕組みを使ってページを
//! 割り当てる。

use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use x86_64::{
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
    PhysAddr, VirtAddr,
};

/// 有効なページテーブルへの参照を返す。
///
/// # Safety
/// 呼び出し側は、渡す`physical_memory_offset`が正しい値(ブートローダーが
/// 実際に物理メモリ全体をマッピングした先頭アドレス)であることを保証する
/// 必要がある。また、この関数は一度しか呼んではならない
/// (`&mut`参照の重複を防ぐため)。
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = active_level_4_table(physical_memory_offset);
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    // CR3レジスタには、現在アクティブなページテーブル(レベル4)の
    // 物理アドレスが入っている。
    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr
}

/// ブートローダーが提供するメモリマップから、実際に空いている
/// (使用可能な)フレームだけを使い回すシンプルなフレームアロケータ。
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize,
}

impl BootInfoFrameAllocator {
    /// # Safety
    /// 呼び出し側は、渡す`memory_map`が正しい(ブートローダーが実際に
    /// 検出した物理メモリの状態を表している)ことを保証する必要がある。
    /// 特に、`Usable`とマークされた領域が本当に未使用であることが前提。
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        BootInfoFrameAllocator {
            memory_map,
            next: 0,
        }
    }

    /// メモリマップの中から、使用可能な4KiBフレームだけを取り出すイテレータ。
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        let regions = self.memory_map.iter();
        let usable_regions = regions.filter(|r| r.region_type == MemoryRegionType::Usable);
        let addr_ranges = usable_regions.map(|r| r.range.start_addr()..r.range.end_addr());
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}