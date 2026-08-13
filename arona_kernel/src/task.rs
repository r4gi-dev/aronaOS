//! タスクの定義

use crate::context::{init_task_stack, TaskContext};
use crate::serial_println;
use alloc::vec;
use alloc::vec::Vec;

const STACK_SIZE: usize = 4096 * 4;

pub struct Task {
    pub context: TaskContext,
    _stack: Vec<u8>,
}

impl Task {
    pub fn new(entry: extern "C" fn() -> !) -> Self {
        let stack = vec![0u8; STACK_SIZE];
        let stack_top = stack.as_ptr() as u64 + STACK_SIZE as u64;
        let context = unsafe { init_task_stack(stack_top, entry) };
        Task {
            context,
            _stack: stack,
        }
    }
}

/// デモ用タスク1。定期的に生存報告をしながらループし続ける。
pub extern "C" fn demo_task_one() -> ! {
    // switch_toは`ret`で終わるため、割り込みハンドラが本来行うはずの
    // 割り込みフラグの復元(iretq相当の処理)が行われない。タスクが
    // 初めて動き出した直後に、自分で明示的に割り込みを再度有効化する。
    x86_64::instructions::interrupts::enable();

    let mut counter: u64 = 0;
    loop {
        counter += 1;
        if counter % 50_000_000 == 0 {
            serial_println!("[Task 1] alive, counter={}", counter);
        }
    }
}

/// デモ用タスク2。タスク1と交互にスケジューリングされる様子を確認する。
pub extern "C" fn demo_task_two() -> ! {
    x86_64::instructions::interrupts::enable();

    let mut counter: u64 = 0;
    loop {
        counter += 1;
        if counter % 50_000_000 == 0 {
            serial_println!("[Task 2] alive, counter={}", counter);
        }
    }
}