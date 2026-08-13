//! RDRAND命令によるハードウェア乱数生成
//!
//! RDRANDは比較的新しいCPU命令で、全てのCPU(や仮想CPU設定)が対応している
//! とは限らない。CPUID命令で対応状況を確認してから使い、対応していない
//! 場合は安全側にフォールバックする(未対応命令の実行による例外の連鎖・
//! ダブルフォルトを避けるため)。

use core::arch::x86_64::{__cpuid, _rdrand64_step, _rdtsc};

/// CPUID(命令リーフ1、ECXレジスタのbit 30)を見て、RDRAND命令に
/// 対応しているCPUかどうかを確認する。
fn rdrand_supported() -> bool {
    unsafe {
        let result = __cpuid(1);
        (result.ecx & (1 << 30)) != 0
    }
}

/// 64bitの乱数を1つ生成する。
///
/// RDRAND対応CPUなら、それを使う(失敗することがある仕様のため数回リトライ)。
/// 非対応の場合は、CPUのタイムスタンプカウンタ(RDTSC、起動からの経過
/// クロック数)を代替として使う。真の乱数ではないが、骨組み段階では
/// 「起動のたびに変わる値」として十分であり、少なくとも安全に動作する。
pub fn random_u64() -> u64 {
    if rdrand_supported() {
        for _ in 0..10 {
            let mut value: u64 = 0;
            let success = unsafe { _rdrand64_step(&mut value) };
            if success == 1 {
                return value;
            }
        }
    }
    unsafe { _rdtsc() }
}