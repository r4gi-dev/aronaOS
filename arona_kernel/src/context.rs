//! タスクの実行状態(コンテキスト)とコンテキストスイッチ
//!
//! 「今CPUが何をしているか」を表すレジスタの値のうち、callee-savedレジスタ
//! (呼び出された側の関数が、値を変える前に保存しておく責任を持つレジスタ:
//! rbp, rbx, r12〜r15)だけを、タスク専用のスタックに保存・復元する方式。
//! これらのレジスタさえ保存すれば「呼び出し元は何も変わっていないはず」と
//! 信じて処理を継続できる、というCPUの呼び出し規約(System V ABI)を利用する。

use core::arch::naked_asm;

/// 1つのタスクの実行状態。実際に保存する値はスタックポインタ(rsp)1つだけ。
/// callee-savedレジスタそのものは、切り替えのたびにスタック上に
/// push/popされるため、この構造体で個別に覚えておく必要はない。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TaskContext {
    pub rsp: u64,
}

impl TaskContext {
    pub const fn empty() -> Self {
        TaskContext { rsp: 0 }
    }
}

/// 新しいタスク用のスタックの中身を初期化する。
///
/// タスクはまだ一度も実行されていないが、初めて`switch_to`で切り替えられた
/// 瞬間に`entry`関数から実行が始まるよう、あらかじめスタックに
/// 「あたかも一度中断されたかのような」偽の保存状態を積んでおく。
///
/// # Safety
/// `stack_top`は、実際に確保された有効なスタック領域の一番上(高位アドレス、
/// 16バイト境界が望ましい)を指している必要がある。
pub unsafe fn init_task_stack(stack_top: u64, entry: extern "C" fn() -> !) -> TaskContext {
    let mut sp = stack_top as *mut u64;

    // switch_toが最後に`ret`した時に読む「戻り先」として、entry関数の
    // アドレスを積んでおく。初回切り替え時、switch_toがretした瞬間に
    // entry関数へジャンプすることになる。
    sp = sp.offset(-1);
    *sp = entry as u64;

    // switch_toがpopする6つのcallee-savedレジスタ分、ダミーの初期値
    // (0)を積んでおく。まだ一度も実行していないタスクなので中身は
    // 何でもよい。
    for _ in 0..6 {
        sp = sp.offset(-1);
        *sp = 0;
    }

    TaskContext { rsp: sp as u64 }
}

/// 現在の実行状態を`current`に保存し、`next`の実行状態へ切り替える。
///
/// # Safety
/// `current`・`next`はどちらも有効な`TaskContext`を指している必要がある。
/// 通常のRustの安全性チェックが及ばない、アセンブリ直書きの領域。
#[unsafe(naked)]
pub unsafe extern "C" fn switch_to(current: *mut TaskContext, next: *const TaskContext) {
    naked_asm!(
        // 現在のcallee-savedレジスタを、今のスタックに保存する
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        // 現在のスタックポインタを、currentが指すTaskContextに保存する。
        // (System V ABIの呼び出し規約上、第1引数はrdiに入っている)
        "mov [rdi], rsp",
        // nextが指すTaskContextからスタックポインタを読み込み、実際に
        // rspを切り替える。これが「切り替え」の本質的な1行。
        // (第2引数はrsiに入っている)
        "mov rsp, [rsi]",
        // 切り替え後のスタックから、保存されていたcallee-savedレジスタを
        // 復元する(pushと逆順で)
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        // retすると、スタックの先頭に積まれているアドレスへジャンプする。
        // 初回切り替え時はinit_task_stackで積んだentry関数、2回目以降は
        // 前回このswitch_to内で中断した箇所に戻る。
        "ret",
    );
}