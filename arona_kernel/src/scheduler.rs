//! ラウンドロビン方式のスケジューラ
//!
//! タスクの実行状態(TaskContext)を順番に並べておき、タイマー割り込みの
//! たびに「次のタスクへ」切り替えていく、最も単純な公平スケジューリング方式。

use crate::context::{switch_to, TaskContext};
use alloc::vec::Vec;
use spin::Mutex;

pub struct Scheduler {
    contexts: Vec<TaskContext>,
    current: usize,
}

static SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);

/// スケジューラを初期化する。カーネル起動時、今動いている実行の流れ
/// (kernel_main自身)を「タスク0」として登録するところから始める。
pub fn init() {
    let mut scheduler = Scheduler {
        contexts: Vec::new(),
        current: 0,
    };
    // kernel_main自身の実行状態を入れる箱を用意する。中身(rsp)は
    // 最初にschedule()が呼ばれた瞬間、switch_toによって書き込まれる。
    scheduler.contexts.push(TaskContext::empty());
    *SCHEDULER.lock() = Some(scheduler);
}

/// 新しいタスクを登録する。
pub fn spawn(context: TaskContext) {
    let mut guard = SCHEDULER.lock();
    let scheduler = guard.as_mut().expect("スケジューラが未初期化です");
    scheduler.contexts.push(context);
}

/// 次のタスクへ切り替える。タイマー割り込みハンドラから呼ばれる想定。
///
/// 重要な注意点: Mutexのロックを保持したまま`switch_to`を呼んではならない。
/// 切り替え先のタスク(あるいはいずれこのタスクに戻ってきた後)が同じ
/// SCHEDULERを再度ロックしようとした瞬間、ロックが解放されないままに
/// なりデッドロックする。そのため、必要な情報(ポインタ)だけ取り出したら
/// 即座にロックを手放し、その後で`switch_to`を呼ぶ。
pub fn schedule() {
    let (current_ptr, next_ptr) = {
        let mut guard = SCHEDULER.lock();
        let scheduler = match guard.as_mut() {
            Some(s) => s,
            None => return, // まだ初期化されていなければ何もしない
        };
        if scheduler.contexts.len() < 2 {
            return; // 切り替え先がまだない
        }
        let current = scheduler.current;
        let next = (current + 1) % scheduler.contexts.len();
        scheduler.current = next;

        let current_ptr = &mut scheduler.contexts[current] as *mut TaskContext;
        let next_ptr = &scheduler.contexts[next] as *const TaskContext;
        (current_ptr, next_ptr)
        // ここでMutexGuard(guard)がスコープを抜けてロック解放される
    };

    unsafe {
        switch_to(current_ptr, next_ptr);
    }
}