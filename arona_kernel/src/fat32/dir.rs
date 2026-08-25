//! FAT32ディレクトリエントリ・ファイル操作
//!
//! フォーマット直後のディスクに対して、実際にファイルを作成・書き込み・
//! 読み込みする最小限の実装。8.3形式のファイル名、ルートディレクトリへの
//! 直接書き込み(サブディレクトリは未対応)、FATチェーンによるクラスタ
//! 割り当てを扱う。
//!
//! 骨組み段階の制約:
//! - ルートディレクトリは1クラスタ分(SECTORS_PER_CLUSTERセクタ)しか走査しない
//!   (ルートディレクトリの拡張・サブディレクトリは未対応)
//! - 長いファイル名(LFN)は未対応。8.3形式に切り詰められる
//! - 空きクラスタ探索は先頭から順に走査する単純な線形探索
//!   (FSInfoのヒント値は骨組み段階では利用しない)

use crate::ata;
use crate::fat32::format::{
    bytes_to_words, cluster_to_sector, words_to_bytes, BYTES_PER_SECTOR, DATA_START_SECTOR,
    FAT_START_SECTOR, NUM_FATS, ROOT_CLUSTER, SECTORS_PER_CLUSTER, SECTORS_PER_FAT,
    TOTAL_SECTORS,
};
use alloc::vec::Vec;

/// 1クラスタのバイト数
const CLUSTER_SIZE: usize = BYTES_PER_SECTOR as usize * SECTORS_PER_CLUSTER as usize;

/// FAT32のエントリはチェーン終端をこの値(以上)で表す
const FAT_EOC: u32 = 0x0FFF_FFFF;
/// 空きクラスタを表す値
const FAT_FREE: u32 = 0x0000_0000;
/// 32バイトのディレクトリエントリ1件あたりのサイズ
const DIR_ENTRY_SIZE: usize = 32;

/// ファイル名を8.3形式(11バイト、大文字、スペース埋め)に変換する。
/// 拡張子付き("REPORT.TXT")・拡張子なし("README")どちらも受け付ける。
/// 8文字・3文字の上限を超える部分は切り詰める(骨組み段階の簡易実装、
/// 長いファイル名(LFN)には未対応。非ASCII文字も想定していない)。
fn to_short_name(name: &str) -> [u8; 11] {
    let mut short = [b' '; 11];
    let (base, ext) = match name.rsplit_once('.') {
        Some((b, e)) => (b, e),
        None => (name, ""),
    };
    for (i, c) in base.chars().take(8).enumerate() {
        short[i] = c.to_ascii_uppercase() as u8;
    }
    for (i, c) in ext.chars().take(3).enumerate() {
        short[8 + i] = c.to_ascii_uppercase() as u8;
    }
    short
}

/// FATテーブルから、指定クラスタが指す次クラスタ値を読み取る。
/// FAT32のエントリは4バイトだが、上位4bitは予約のためマスクする。
fn read_fat_entry(cluster: u32) -> Result<u32, &'static str> {
    let fat_offset = cluster as u64 * 4;
    let sector = FAT_START_SECTOR as u64 + fat_offset / BYTES_PER_SECTOR as u64;
    let offset_in_sector = (fat_offset % BYTES_PER_SECTOR as u64) as usize;

    let mut words = [0u16; 256];
    ata::read_sector(sector as u32, &mut words)?;
    let bytes = words_to_bytes(&words);

    let raw = u32::from_le_bytes([
        bytes[offset_in_sector],
        bytes[offset_in_sector + 1],
        bytes[offset_in_sector + 2],
        bytes[offset_in_sector + 3],
    ]);
    Ok(raw & 0x0FFF_FFFF)
}

/// FATテーブルに、指定クラスタの次クラスタ値を書き込む。
/// 冗長化のため、両方のFATコピー(NUM_FATS=2)に同じ値を書く。
fn write_fat_entry(cluster: u32, value: u32) -> Result<(), &'static str> {
    let fat_offset = cluster as u64 * 4;
    let sector_in_fat = fat_offset / BYTES_PER_SECTOR as u64;
    let offset_in_sector = (fat_offset % BYTES_PER_SECTOR as u64) as usize;

    for fat_index in 0..NUM_FATS as u64 {
        let fat_start = FAT_START_SECTOR as u64 + fat_index * SECTORS_PER_FAT as u64;
        let sector = fat_start + sector_in_fat;

        let mut words = [0u16; 256];
        ata::read_sector(sector as u32, &mut words)?;
        let mut bytes = words_to_bytes(&words);

        // 上位4bitは予約のため、既存の値を保持したまま下位28bitだけ書き換える
        let existing = u32::from_le_bytes([
            bytes[offset_in_sector],
            bytes[offset_in_sector + 1],
            bytes[offset_in_sector + 2],
            bytes[offset_in_sector + 3],
        ]);
        let new_value = (existing & 0xF000_0000) | (value & 0x0FFF_FFFF);
        let new_bytes = new_value.to_le_bytes();
        bytes[offset_in_sector..offset_in_sector + 4].copy_from_slice(&new_bytes);

        let new_words = bytes_to_words(&bytes);
        ata::write_sector(sector as u32, &new_words)?;
    }
    Ok(())
}

/// 空きクラスタを1つ探して返す(先頭から順に走査する簡易実装)。
fn find_free_cluster() -> Result<u32, &'static str> {
    let total_clusters = (TOTAL_SECTORS - DATA_START_SECTOR) / SECTORS_PER_CLUSTER as u32;
    for cluster in 2..(2 + total_clusters) {
        if read_fat_entry(cluster)? == FAT_FREE {
            return Ok(cluster);
        }
    }
    Err("空きクラスタが見つかりません(ディスクフル)")
}

/// 必要なバイト数を格納できるだけのクラスタチェーンを新規に割り当て、
/// 先頭クラスタ番号を返す。
fn allocate_cluster_chain(byte_len: usize) -> Result<u32, &'static str> {
    let needed_clusters = byte_len.div_ceil(CLUSTER_SIZE).max(1);

    let first_cluster = find_free_cluster()?;
    write_fat_entry(first_cluster, FAT_EOC)?; // 1クラスタだけの場合はこれが最終形

    let mut prev = first_cluster;
    for _ in 1..needed_clusters {
        let next = find_free_cluster()?;
        write_fat_entry(prev, next)?;
        write_fat_entry(next, FAT_EOC)?;
        prev = next;
    }
    Ok(first_cluster)
}

/// データをクラスタチェーンに書き込む。クラスタ端数はゼロ埋めされる
/// (書き込み前にセクタバッファをゼロクリアしているため)。
fn write_data_to_chain(first_cluster: u32, data: &[u8]) -> Result<(), &'static str> {
    let mut cluster = first_cluster;
    let mut offset = 0;

    while offset < data.len() {
        let sector_start = cluster_to_sector(cluster);
        let chunk_end = (offset + CLUSTER_SIZE).min(data.len());
        let chunk = &data[offset..chunk_end];

        for sector_index in 0..SECTORS_PER_CLUSTER as usize {
            let mut sector_bytes = [0u8; 512];
            let start = sector_index * BYTES_PER_SECTOR as usize;
            if start < chunk.len() {
                let end = (start + BYTES_PER_SECTOR as usize).min(chunk.len());
                sector_bytes[..end - start].copy_from_slice(&chunk[start..end]);
            }
            let words = bytes_to_words(&sector_bytes);
            ata::write_sector(sector_start + sector_index as u32, &words)?;
        }

        offset += CLUSTER_SIZE;
        if offset < data.len() {
            cluster = read_fat_entry(cluster)?;
        }
    }
    Ok(())
}

/// クラスタチェーンからデータを読み込む(file_sizeバイト分)。
fn read_data_from_chain(first_cluster: u32, file_size: usize) -> Result<Vec<u8>, &'static str> {
    let mut data = Vec::with_capacity(file_size);
    let mut cluster = first_cluster;

    while data.len() < file_size {
        let sector_start = cluster_to_sector(cluster);
        for sector_index in 0..SECTORS_PER_CLUSTER as usize {
            let mut words = [0u16; 256];
            ata::read_sector(sector_start + sector_index as u32, &mut words)?;
            let bytes = words_to_bytes(&words);
            let remaining = file_size - data.len();
            let take = remaining.min(BYTES_PER_SECTOR as usize);
            data.extend_from_slice(&bytes[..take]);
            if data.len() >= file_size {
                break;
            }
        }
        if data.len() < file_size {
            cluster = read_fat_entry(cluster)?;
        }
    }
    Ok(data)
}

/// ルートディレクトリ内に、指定した名前・データでファイルを1件書き込む。
///
/// 空のファイル(data.len() == 0)は、クラスタを1つ確保した上でゼロバイトの
/// ファイルサイズとして記録する(FAT32仕様上、空ファイルにも先頭クラスタを
/// 割り当てておく実装は許容されている)。
pub fn write_file(name: &str, data: &[u8]) -> Result<(), &'static str> {
    let short_name = to_short_name(name);

    let first_cluster = allocate_cluster_chain(data.len())?;
    write_data_to_chain(first_cluster, data)?;

    let root_dir_start = cluster_to_sector(ROOT_CLUSTER);
    for sector_offset in 0..SECTORS_PER_CLUSTER as u32 {
        let sector = root_dir_start + sector_offset;
        let mut words = [0u16; 256];
        ata::read_sector(sector, &mut words)?;
        let mut bytes = words_to_bytes(&words);

        for entry_index in 0..(BYTES_PER_SECTOR as usize / DIR_ENTRY_SIZE) {
            let entry_start = entry_index * DIR_ENTRY_SIZE;
            let first_byte = bytes[entry_start];
            // 0x00: 未使用領域の先頭(以降も全て未使用) / 0xE5: 削除済みエントリ
            if first_byte == 0x00 || first_byte == 0xE5 {
                bytes[entry_start..entry_start + 11].copy_from_slice(&short_name);
                bytes[entry_start + 11] = 0x20; // ATTR_ARCHIVE(通常ファイル)
                bytes[entry_start + 20..entry_start + 22]
                    .copy_from_slice(&((first_cluster >> 16) as u16).to_le_bytes());
                bytes[entry_start + 26..entry_start + 28]
                    .copy_from_slice(&((first_cluster & 0xFFFF) as u16).to_le_bytes());
                bytes[entry_start + 28..entry_start + 32]
                    .copy_from_slice(&(data.len() as u32).to_le_bytes());

                let new_words = bytes_to_words(&bytes);
                ata::write_sector(sector, &new_words)?;
                return Ok(());
            }
        }
    }
    Err("ルートディレクトリに空きエントリがありません")
}

/// ルートディレクトリから指定した名前のファイルを探し、内容を読み込む。
pub fn read_file(name: &str) -> Result<Vec<u8>, &'static str> {
    let short_name = to_short_name(name);

    let root_dir_start = cluster_to_sector(ROOT_CLUSTER);
    for sector_offset in 0..SECTORS_PER_CLUSTER as u32 {
        let sector = root_dir_start + sector_offset;
        let mut words = [0u16; 256];
        ata::read_sector(sector, &mut words)?;
        let bytes = words_to_bytes(&words);

        for entry_index in 0..(BYTES_PER_SECTOR as usize / DIR_ENTRY_SIZE) {
            let entry_start = entry_index * DIR_ENTRY_SIZE;
            let first_byte = bytes[entry_start];
            if first_byte == 0x00 {
                break; // これ以降は未使用領域
            }
            if first_byte == 0xE5 {
                continue; // 削除済み
            }
            if bytes[entry_start..entry_start + 11] == short_name {
                let cluster_high =
                    u16::from_le_bytes([bytes[entry_start + 20], bytes[entry_start + 21]]) as u32;
                let cluster_low =
                    u16::from_le_bytes([bytes[entry_start + 26], bytes[entry_start + 27]]) as u32;
                let first_cluster = (cluster_high << 16) | cluster_low;
                let file_size = u32::from_le_bytes([
                    bytes[entry_start + 28],
                    bytes[entry_start + 29],
                    bytes[entry_start + 30],
                    bytes[entry_start + 31],
                ]) as usize;

                return read_data_from_chain(first_cluster, file_size);
            }
        }
    }
    Err("指定された名前のファイルが見つかりません")
}