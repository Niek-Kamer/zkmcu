//! RP2350 bring-up: XOSC + PLL + USB allocator.
//!
//! `init_rp2350` consumes `hal::pac::Peripherals`, configures the
//! 150 MHz system clock, takes the timer and USB peripherals, and
//! returns the pieces that outlive the main function. The clocks
//! object itself is dropped — no caller currently needs to tweak
//! peripheral clocks after bring-up.

use rp235x_hal as hal;
use usb_device::class_prelude::UsbBusAllocator;

use crate::usb::Timer0;

pub const XTAL_HZ: u32 = 12_000_000;
pub const SYS_HZ: u32 = 150_000_000;

/// Configure clocks and take Timer0 + USB.
///
/// Hands back the pair the binary's `main` wants to keep in scope.
/// Panics on clock-init failure: there is no useful recovery path for
/// a PLL that won't lock.
pub fn init_rp2350(mut pac: hal::pac::Peripherals) -> (Timer0, UsbBusAllocator<hal::usb::UsbBus>) {
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);
    let Ok(clocks) = hal::clocks::init_clocks_and_plls(
        XTAL_HZ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    ) else {
        panic!("clock init");
    };
    // Assert the configured system clock matches `SYS_HZ`. Every cycles->us
    // conversion across the bench harness divides by `SYS_HZ` as a constant,
    // so a silent drift in the rp235x-hal default would corrupt every
    // published timing without visible failure. Panic at boot instead.
    let actual_hz = <hal::clocks::SystemClock as hal::clocks::Clock>::freq(&clocks.system_clock);
    assert!(actual_hz.to_Hz() == SYS_HZ, "SYS_HZ mismatch");
    let timer = hal::Timer::new_timer0(pac.TIMER0, &mut pac.RESETS, &clocks);
    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USB,
        pac.USB_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));
    (timer, usb_bus)
}
