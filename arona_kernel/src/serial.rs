//! シリアルポート(COM1)経由のテキスト出力。
//!
//! QEMU上での開発では、VGA画面より圧倒的にログが見やすく・追いやすい。
//! 将来的なデバッグ・自動テストの土台としても、この段階で整備しておく価値がある。

use core::fmt;
use core::fmt::Write;
use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;

lazy_static! {
    /// COM1の標準的なI/Oポートアドレス(0x3F8)を使う。
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    SERIAL1
        .lock()
        .write_fmt(args)
        .expect("シリアルポートへの出力に失敗しました");
}

/// シリアルポートへ出力する(改行なし)
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*));
    };
}

/// シリアルポートへ出力する(改行あり)
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(
        concat!($fmt, "\n"), $($arg)*));
}