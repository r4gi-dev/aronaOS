//! FAT32フォーマット(ディスクへの初期書き込み)
//!
//! ディスクサイズ・クラスタサイズなどは、512MBのディスクに対して
//! 事前に手計算した固定値を使う(実行時の動的計算は複雑になりすぎるため、
//! 骨組み段階ではこの1構成に決め打ちする)。

use crate::ata;
use crate::random;

pub const BYTES_PER_SECTOR: u16 = 512;
pub const SECTORS_PER_CLUSTER: u8 = 8; // 4096バイト = 4KBクラスタ
pub const RESERVED_SECTORS: u16 = 32;
pub const NUM_FATS: u8 = 2;
pub const TOTAL_SECTORS: u32 = 1_048_576; // 512MiB / 512バイト
pub const SECTORS_PER_FAT: u32 = 1024;
pub const ROOT_CLUSTER: u32 = 2;

pub const FAT_START_SECTOR: u32 = RESERVED_SECTORS as u32;
pub const DATA_START_SECTOR: u32 = FAT_START_SECTOR + (NUM_FATS as u32 * SECTORS_PER_FAT);

/// クラスタ番号から、それが実際にディスク上のどのセクタから始まるかを計算する。
/// FAT32の仕様上、クラスタ番号は2から始まる(0と1は予約されている)。
pub fn cluster_to_sector(cluster: u32) -> u32 {
    DATA_START_SECTOR + (cluster - 2) * SECTORS_PER_CLUSTER as u32
}

/// 512バイトのバイト列を、ATAドライバが扱う256ワード(16bit)配列に変換する。
/// `dir.rs`のディレクトリエントリ・FAT操作でも同じ変換が必要なため、
/// `pub(crate)`にしてクレート内で共有する。
pub(crate) fn bytes_to_words(bytes: &[u8; 512]) -> [u16; 256] {
    let mut words = [0u16; 256];
    for i in 0..256 {
        words[i] = (bytes[i * 2] as u16) | ((bytes[i * 2 + 1] as u16) << 8);
    }
    words
}

/// `bytes_to_words`の逆変換。
pub(crate) fn words_to_bytes(words: &[u16; 256]) -> [u8; 512] {
    let mut bytes = [0u8; 512];
    for i in 0..256 {
        bytes[i * 2] = (words[i] & 0xFF) as u8;
        bytes[i * 2 + 1] = (words[i] >> 8) as u8;
    }
    bytes
}

fn write_boot_sector() -> Result<(), &'static str> {
    let mut sector = [0u8; 512];

    sector[0] = 0xEB;
    sector[1] = 0x58;
    sector[2] = 0x90;
    sector[3..11].copy_from_slice(b"ARONAOS ");

    sector[11..13].copy_from_slice(&BYTES_PER_SECTOR.to_le_bytes());
    sector[13] = SECTORS_PER_CLUSTER;
    sector[14..16].copy_from_slice(&RESERVED_SECTORS.to_le_bytes());
    sector[16] = NUM_FATS;
    sector[17..19].copy_from_slice(&0u16.to_le_bytes());
    sector[19..21].copy_from_slice(&0u16.to_le_bytes());
    sector[21] = 0xF8;
    sector[22..24].copy_from_slice(&0u16.to_le_bytes());
    sector[24..26].copy_from_slice(&32u16.to_le_bytes());
    sector[26..28].copy_from_slice(&64u16.to_le_bytes());
    sector[28..32].copy_from_slice(&0u32.to_le_bytes());
    sector[32..36].copy_from_slice(&TOTAL_SECTORS.to_le_bytes());
    sector[36..40].copy_from_slice(&SECTORS_PER_FAT.to_le_bytes());
    sector[40..42].copy_from_slice(&0u16.to_le_bytes());
    sector[42..44].copy_from_slice(&0u16.to_le_bytes());
    sector[44..48].copy_from_slice(&ROOT_CLUSTER.to_le_bytes());
    sector[48..50].copy_from_slice(&1u16.to_le_bytes());
    sector[50..52].copy_from_slice(&6u16.to_le_bytes());
    sector[64] = 0x80;
    sector[65] = 0;
    sector[66] = 0x29;
    let volume_id = random::random_u64() as u32;
    sector[67..71].copy_from_slice(&volume_id.to_le_bytes());
    sector[71..82].copy_from_slice(b"ARONAOS    ");
    sector[82..90].copy_from_slice(b"FAT32   ");

    sector[510] = 0x55;
    sector[511] = 0xAA;

    let words = bytes_to_words(&sector);
    ata::write_sector(0, &words)?;
    ata::write_sector(6, &words)?; // バックアップブートセクタ(仕様上の複製)
    Ok(())
}

fn write_fsinfo_sector() -> Result<(), &'static str> {
    let mut sector = [0u8; 512];
    sector[0..4].copy_from_slice(&0x41615252u32.to_le_bytes());
    sector[484..488].copy_from_slice(&0x61417272u32.to_le_bytes());
    // 空きクラスタ数・次の空きクラスタのヒントは「不明」を意味する
    // 0xFFFFFFFFにしておく(仕様上、OS側はこの値を信用せず再計算してもよいことになっている)
    sector[488..492].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    sector[492..496].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    sector[508..512].copy_from_slice(&0xAA550000u32.to_le_bytes());

    let words = bytes_to_words(&sector);
    ata::write_sector(1, &words)?;
    ata::write_sector(7, &words)?; // バックアップ
    Ok(())
}

fn init_fat_tables() -> Result<(), &'static str> {
    let zero_sector = [0u16; 256];

    for fat_index in 0..NUM_FATS as u32 {
        let fat_start = FAT_START_SECTOR + (fat_index * SECTORS_PER_FAT);
        for offset in 0..SECTORS_PER_FAT {
            ata::write_sector(fat_start + offset, &zero_sector)?;
        }
    }

    // FAT表の先頭3エントリは仕様上の予約値を書き込む必要がある
    let mut first_sector = [0u8; 512];
    first_sector[0..4].copy_from_slice(&0x0FFFFFF8u32.to_le_bytes()); // FAT[0]: メディア記述子
    first_sector[4..8].copy_from_slice(&0x0FFFFFFFu32.to_le_bytes()); // FAT[1]: 予約
    // FAT[2]: ルートディレクトリの開始クラスタ。まだ1クラスタだけなので
    // 「ここでチェーン終了」を示すマーカーにしておく。
    first_sector[8..12].copy_from_slice(&0x0FFFFFFFu32.to_le_bytes());

    let words = bytes_to_words(&first_sector);
    for fat_index in 0..NUM_FATS as u32 {
        let fat_start = FAT_START_SECTOR + (fat_index * SECTORS_PER_FAT);
        ata::write_sector(fat_start, &words)?;
    }

    Ok(())
}

fn init_root_directory() -> Result<(), &'static str> {
    let zero_sector = [0u16; 256];
    let root_dir_start = cluster_to_sector(ROOT_CLUSTER);
    for offset in 0..SECTORS_PER_CLUSTER as u32 {
        ata::write_sector(root_dir_start + offset, &zero_sector)?;
    }
    Ok(())
}

/// ディスク全体をFAT32としてフォーマットする(初期化する)。
/// ブートセクタ・FSInfoセクタ・FAT表2つ・ルートディレクトリの順で書き込む。
pub fn format() -> Result<(), &'static str> {
    crate::serial_println!("FAT32: formatting disk (this may take a moment)...");
    write_boot_sector()?;
    write_fsinfo_sector()?;
    init_fat_tables()?;
    init_root_directory()?;
    crate::serial_println!("FAT32: format complete.");
    Ok(())
}