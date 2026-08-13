//! ATA(IDE) PIO方式によるディスクドライバ
//!
//! CPUが特定のI/Oポートに値を書き込む・読み出すことで、ディスクと
//! 直接やり取りする、古典的だが実装がシンプルな方式。セカンダリATA
//! チャンネル(データ保存用ディスクを繋いだ側)を対象にする。

use x86_64::instructions::port::Port;

const DATA: u16 = 0x170;
const SECTOR_COUNT: u16 = 0x172;
const LBA_LOW: u16 = 0x173;
const LBA_MID: u16 = 0x174;
const LBA_HIGH: u16 = 0x175;
const DRIVE_HEAD: u16 = 0x176;
const STATUS_COMMAND: u16 = 0x177;

const CMD_READ_SECTORS: u8 = 0x20;
const CMD_WRITE_SECTORS: u8 = 0x30;
const CMD_CACHE_FLUSH: u8 = 0xE7;

const STATUS_BSY: u8 = 0x80;
const STATUS_DRQ: u8 = 0x08;
const STATUS_ERR: u8 = 0x01;

fn wait_not_busy() {
    let mut status_port: Port<u8> = Port::new(STATUS_COMMAND);
    loop {
        let status = unsafe { status_port.read() };
        if status & STATUS_BSY == 0 {
            break;
        }
    }
}

fn wait_drq() -> Result<(), &'static str> {
    let mut status_port: Port<u8> = Port::new(STATUS_COMMAND);
    loop {
        let status = unsafe { status_port.read() };
        if status & STATUS_ERR != 0 {
            return Err("ATAディスクがエラーを報告しました");
        }
        if status & STATUS_DRQ != 0 {
            return Ok(());
        }
    }
}

unsafe fn select_sector(lba: u32) {
    let mut drive_head: Port<u8> = Port::new(DRIVE_HEAD);
    drive_head.write(0xE0 | (((lba >> 24) & 0x0F) as u8));

    let mut sector_count: Port<u8> = Port::new(SECTOR_COUNT);
    sector_count.write(1u8);

    let mut lba_low: Port<u8> = Port::new(LBA_LOW);
    lba_low.write((lba & 0xFF) as u8);
    let mut lba_mid: Port<u8> = Port::new(LBA_MID);
    lba_mid.write(((lba >> 8) & 0xFF) as u8);
    let mut lba_high: Port<u8> = Port::new(LBA_HIGH);
    lba_high.write(((lba >> 16) & 0xFF) as u8);
}

pub fn read_sector(lba: u32, buffer: &mut [u16; 256]) -> Result<(), &'static str> {
    wait_not_busy();
    unsafe {
        select_sector(lba);
        let mut command: Port<u8> = Port::new(STATUS_COMMAND);
        command.write(CMD_READ_SECTORS);
    }
    wait_drq()?;

    let mut data_port: Port<u16> = Port::new(DATA);
    for word in buffer.iter_mut() {
        *word = unsafe { data_port.read() };
    }
    Ok(())
}

pub fn write_sector(lba: u32, buffer: &[u16; 256]) -> Result<(), &'static str> {
    wait_not_busy();
    unsafe {
        select_sector(lba);
        let mut command: Port<u8> = Port::new(STATUS_COMMAND);
        command.write(CMD_WRITE_SECTORS);
    }
    wait_drq()?;

    let mut data_port: Port<u16> = Port::new(DATA);
    for &word in buffer.iter() {
        unsafe { data_port.write(word) };
    }

    wait_not_busy();
    unsafe {
        let mut command: Port<u8> = Port::new(STATUS_COMMAND);
        command.write(CMD_CACHE_FLUSH);
    }
    wait_not_busy();
    Ok(())
}