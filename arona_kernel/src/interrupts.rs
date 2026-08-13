//! 割り込み処理(IDT: Interrupt Descriptor Table)
//!
//! CPU例外(ブレークポイント・ダブルフォルト)に加え、今回からハードウェア
//! 割り込み(タイマー)も扱う。ハードウェア割り込みはPIC(割り込み制御チップ)
//! を経由してCPUに届くため、まずPICの初期化(リマップ)が必要になる。

use crate::serial_println;
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

/// PICは2つのチップが連結されている(親と子)。親のリマップ先を32番から、
/// 子は親の8個後ろ(40番)から始める、というのが伝統的な配置。
/// 0〜31番はCPU例外(ゼロ除算・ページフォルト等)が占有しているため、
/// それより後ろに割り当てる。
pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

/// PICの制御はハードウェアとの直接やり取りなので、同時に複数の場所から
/// 触ると壊れる。Mutexで保護しておく。
pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

/// リマップ後の割り込み番号一覧。タイマーは親PICの0番目の線(IRQ0)なので、
/// リマップ後は32番になる。
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }
    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.double_fault.set_handler_fn(double_fault_handler);
        idt[InterruptIndex::Timer.as_u8()].set_handler_fn(timer_interrupt_handler);
        idt
    };
}

/// IDTのロードに加え、PICの初期化(リマップ)とCPUの割り込み受付を有効化する。
pub fn init() {
    IDT.load();

    unsafe {
        PICS.lock().initialize();
    }

    // CPUには「割り込みを受け付けるかどうか」のフラグ(IF)があり、
    // 起動直後はオフになっている。オンにしないと、PICがいくら信号を
    // 送ってもCPUは無視し続ける。
    x86_64::instructions::interrupts::enable();
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

/// タイマー割り込みハンドラ。一定時間ごとに呼ばれる。
///
/// 重要な注意点: このハンドラの最後で必ず「割り込み終了(EOI)」信号を
/// PICに送る必要がある。これを忘れると、PICは「まだ処理中だ」と思い込み、
/// 次のタイマー割り込みを二度と送ってこなくなる。
extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    static TICKS: spin::Mutex<u64> = spin::Mutex::new(0);
    {
        let mut ticks = TICKS.lock();
        *ticks += 1;
        if *ticks % 18 == 0 {
            serial_println!("timer tick: {}", *ticks);
        }
    }

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }

    // EOIを送った後にタスクを切り替える。これにより次のタイマー割り込みは
    // 切り替え先のタスクが動いている間にも正常に発生し続けられる。
    crate::scheduler::schedule();
}