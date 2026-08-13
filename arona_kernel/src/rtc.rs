//! CMOS RTC(リアルタイムクロック)からの時刻取得
//!
//! std環境の`chrono::Utc::now()`は裏でOSに時刻を問い合わせているが、
//! カーネル自身がOSである以上、このハードウェアとのやり取りを
//! 自分で実装する必要がある。CMOS RTCはI/Oポート0x70(インデックス指定用)・
//! 0x71(データ読み書き用)を使う、x86の伝統的な仕組み。

use x86_64::instructions::port::Port;

/// CMOSレジスタから1バイト読み出す
unsafe fn read_cmos_register(register: u8) -> u8 {
    let mut index_port: Port<u8> = Port::new(0x70);
    let mut data_port: Port<u8> = Port::new(0x71);
    index_port.write(register);
    data_port.read()
}

/// RTCが値を更新している最中かどうかを確認する。更新中に読み取ると
/// 値が中途半端になる可能性があるため、更新中でないタイミングを待つ。
unsafe fn update_in_progress() -> bool {
    read_cmos_register(0x0A) & 0x80 != 0
}

/// BCD(Binary-Coded Decimal、1バイトの上位4bitと下位4bitでそれぞれ
/// 十進数1桁を表す、RTCの伝統的な数値表現)を通常の数値に変換する。
fn bcd_to_binary(bcd: u8) -> u8 {
    (bcd & 0x0F) + ((bcd / 16) * 10)
}

#[derive(Debug, Clone, Copy)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// 現在時刻をCMOS RTCから読み取る。
///
/// 骨組み段階の簡易実装: BCD形式かどうか・24時間表記かどうかは環境により
/// 異なるが、QEMUのデフォルト設定(BCD・24時間表記)を前提にしている。
/// 実機対応やタイムゾーン考慮は将来の課題とする。
pub fn now() -> DateTime {
    unsafe {
        // 更新中でないタイミングになるまで待つ
        while update_in_progress() {}

        let second = bcd_to_binary(read_cmos_register(0x00));
        let minute = bcd_to_binary(read_cmos_register(0x02));
        let hour = bcd_to_binary(read_cmos_register(0x04));
        let day = bcd_to_binary(read_cmos_register(0x07));
        let month = bcd_to_binary(read_cmos_register(0x08));
        let year_low = bcd_to_binary(read_cmos_register(0x09));

        DateTime {
            // CMOS RTCは西暦の下2桁しか持たないため、2000年代前提で補完する。
            // (設計まとめ全体が「何十年」スパンを見据えているため、将来
            // 2100年問題ならぬ「CMOS世紀問題」が起きうる点は要注意。
            // 実運用ではACPI経由の世紀レジスタ参照に置き換えるべき)
            year: 2000 + year_low as u16,
            month,
            day,
            hour,
            minute,
            second,
        }
    }
}