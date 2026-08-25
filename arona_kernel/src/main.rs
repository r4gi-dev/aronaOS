//! AronaOS カーネル、最初の一歩。
//!
//! no_std環境で動く最小限のカーネル。起動・シリアル出力・割り込み処理・
//! メモリ管理・プリエンプティブスケジューラに加え、時計(RTC)・乱数
//! (RDRAND)という基礎土台の上に、Guardian・権限テンプレート・信頼モデルの
//! カーネル移植版(試験実装)、FAT32ファイルシステム(フォーマット・
//! ファイル読み書き)が動いている。

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod adaptive;
mod allocator;
mod context;
mod guardian;
mod interrupts;
mod memory;
mod permissions;
mod random;
mod rtc;
mod scheduler;
mod serial;
mod task;
mod fat32;
mod ata;

use alloc::string::String;
use alloc::vec::Vec;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use x86_64::VirtAddr;

static HELLO: &[u8] = b"AronaOS Kernel: Hello, r4gi-san.";

entry_point!(kernel_main);

static mut TASK_STORAGE: Vec<task::Task> = Vec::new();

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    let vga_buffer = 0xb8000 as *mut u8;
    for (i, &byte) in HELLO.iter().enumerate() {
        unsafe {
            *vga_buffer.offset(i as isize * 2) = byte;
            *vga_buffer.offset(i as isize * 2 + 1) = 0xb;
        }
    }

    serial_println!("AronaOS Kernel booted successfully.");

    interrupts::init();
    serial_println!("IDT loaded.");

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };

    allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("ヒープの初期化に失敗しました");
    serial_println!("Heap initialized.");

    let task_one = task::Task::new(task::demo_task_one);
    let task_two = task::Task::new(task::demo_task_two);

    scheduler::init();
    scheduler::spawn(task_one.context);
    scheduler::spawn(task_two.context);

    unsafe {
        #[allow(static_mut_refs)]
        {
            TASK_STORAGE.push(task_one);
            TASK_STORAGE.push(task_two);
        }
    }

    serial_println!("Scheduler initialized with 2 tasks. Entering idle loop.");

    let now = rtc::now();
    serial_println!(
        "RTC time: {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        now.year, now.month, now.day, now.hour, now.minute, now.second
    );

    let random_value = random::random_u64();
    serial_println!("Random u64: {:#018x}", random_value);

    // --- Guardianのカーネル移植版デモ ---
    let guardian_engine = guardian::GuardianEngine::with_default_rules();
    serial_println!("Guardian initialized with {} rules.", guardian_engine.rules().len());

    let suspicious_event = guardian::SystemEvent::FileOperation {
        path: String::from("C:/dev/suspicious/report.docx.locked"),
        operation: String::from("write"),
        recent_write_count: 0,
    };
    let interventions = guardian_engine.evaluate(&suspicious_event);
    if let Some(intervention) = interventions.first() {
        let guardian::InterventionAction::Block { reason } = &intervention.action;
        serial_println!("Guardian BLOCKED: {}", reason);
    }

    // --- 権限テンプレートのカーネル移植版デモ ---
    let template = permissions::rust_dev_environment_template();
    serial_println!("Permission template loaded: {}", template.name);

    let mut grant = permissions::PurposeGrant::new(
        "Rust開発環境を整えたい",
        &template,
        alloc::vec![],
        0, // 起動直後のティックを0とする
    );

    let cargo_capability = permissions::Capability::ProcessExecution {
        program: String::from("cargo.exe"),
    };
    grant
        .expand(&template, cargo_capability, 0)
        .expect("テンプレート範囲内のはずの拡張が失敗しました");
    serial_println!(
        "Permission expanded. Granted capabilities: {}",
        grant.granted_capabilities.len()
    );

    // 大きく時間が経過した(ティック数が進んだ)状況を模擬し、休眠判定を確認する
    let became_dormant = grant.check_dormancy(1000);
    serial_println!(
        "Dormancy check after simulated elapsed time: became_dormant={}",
        became_dormant
    );

    // --- 信頼モデルのカーネル移植版デモ ---
    let mut trust_model = adaptive::TrustModel::new();
    serial_println!(
        "Trust check before any approvals: skip_confirmation={}",
        trust_model.should_skip_confirmation("dev_tooling")
    );

    for _ in 0..5 {
        trust_model.record_approval("dev_tooling", adaptive::ApprovalManner::Immediate);
    }
    serial_println!(
        "Trust check after 5 immediate approvals: skip_confirmation={}",
        trust_model.should_skip_confirmation("dev_tooling")
    );

    serial_println!("All kernel-ported subsystems verified.");

    // --- ATAディスクドライバの動作確認 ---
    let write_buffer: [u16; 256] = {
        let mut buf = [0u16; 256];
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = 0xA500 | (i as u16 & 0xFF);
        }
        buf
    };

    match ata::write_sector(0, &write_buffer) {
        Ok(()) => {
            serial_println!("ATA: wrote test pattern to sector 0.");
        }
        Err(e) => {
            serial_println!("ATA: write failed: {}", e);
        }
    }

    let mut read_buffer = [0u16; 256];
    match ata::read_sector(0, &mut read_buffer) {
        Ok(()) => {
            if read_buffer == write_buffer {
                serial_println!("ATA: read back matches what was written. Disk I/O is working.");
            } else {
                serial_println!("ATA: read back MISMATCH. Something is wrong.");
            }
        }
        Err(e) => {
            serial_println!("ATA: read failed: {}", e);
        }
    }

    // --- FAT32フォーマット ---
    //
    // 数千回のブロッキングATAセクタ書き込みを伴う長時間処理のため、
    // タイマー割り込みによるタスク切り替えが処理の途中に何度も割り込むと、
    // (原因はまだ特定できていないが)ダブルフォルトを引き起こすことを確認した。
    // ディスクI/O中は割り込みを止めるのが定石でもあるため、
    // `without_interrupts`でこの区間全体を保護する。
    let format_result = x86_64::instructions::interrupts::without_interrupts(fat32::format::format);
    match format_result {
        Ok(()) => {
            serial_println!("FAT32 filesystem initialized on data disk.");
        }
        Err(e) => {
            serial_println!("FAT32 format failed: {}", e);
        }
    }

    // --- FAT32 ファイル書き込み・読み込みの動作確認 ---
    // ATAセクタI/Oの確認と同じパターン: 書いたものが正しく読み返せるかを検証する。
    // こちらも同じ理由でwithout_interruptsで保護する。
    let test_file_name = "HELLO.TXT";
    let test_file_content = b"AronaOS FAT32 test: r4gi-san, konnichiwa.";

    let write_result =
        x86_64::instructions::interrupts::without_interrupts(|| fat32::dir::write_file(test_file_name, test_file_content));

    match write_result {
        Ok(()) => {
            serial_println!(
                "FAT32: wrote file '{}' ({} bytes).",
                test_file_name,
                test_file_content.len()
            );

            let read_result =
                x86_64::instructions::interrupts::without_interrupts(|| fat32::dir::read_file(test_file_name));

            match read_result {
                Ok(read_back) => {
                    if read_back == test_file_content {
                        serial_println!(
                            "FAT32: read back matches what was written. File I/O is working."
                        );
                    } else {
                        serial_println!("FAT32: read back MISMATCH. Something is wrong.");
                    }
                }
                Err(e) => {
                    serial_println!("FAT32: read_file failed: {}", e);
                }
            }
        }
        Err(e) => {
            serial_println!("FAT32: write_file failed: {}", e);
        }
    }

    // 存在しないファイルを読もうとした場合、エラーとして扱われることも確認しておく
    // (「黙って空データを返す」ような失敗の握りつぶしを避ける設計方針の確認)
    let missing_result =
        x86_64::instructions::interrupts::without_interrupts(|| fat32::dir::read_file("NOTFOUND.TXT"));
    match missing_result {
        Ok(_) => {
            serial_println!("FAT32: unexpectedly found NOTFOUND.TXT (this should not happen)");
        }
        Err(e) => {
            serial_println!("FAT32: correctly reported missing file: {}", e);
        }
    }

    serial_println!("Entering idle loop.");

    loop {
        x86_64::instructions::hlt();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("KERNEL PANIC: {}", info);
    loop {}
}