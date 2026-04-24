//! CDC-ACM serial console over the RP2350's native USB.
//!
//! `Bench` bundles the three pieces every firmware binary touches on
//! every output line: the `UsbDevice`, the `SerialPort`, and a copy of
//! the `Timer0` handle for deadline-driven polling. `write_line`,
//! `print_marker`, and `print_result` match the formats the existing
//! benchmark log parsers expect, so downstream TOML emission stays
//! drop-in.

use core::fmt::Write as _;

use heapless::String;
use rp235x_hal as hal;
use usb_device::class_prelude::{UsbBus, UsbBusAllocator};
use usb_device::prelude::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid};
use usbd_serial::SerialPort;

pub type Timer0 = hal::Timer<hal::timer::CopyableTimer0>;

/// USB descriptor strings for the device. `serial` is 4 ASCII digits by
/// convention in this project.
#[derive(Clone, Copy)]
pub struct BenchConfig {
    pub manufacturer: &'static str,
    pub product: &'static str,
    pub serial: &'static str,
}

/// Binding of the USB stack + timer that every firmware `main()` uses.
pub struct Bench<'a, B: UsbBus> {
    pub dev: UsbDevice<'a, B>,
    pub serial: SerialPort<'a, B>,
    pub timer: Timer0,
}

impl<'a, B: UsbBus> Bench<'a, B> {
    /// Build the USB device on the 16c0:27dd test VID/PID. The bus
    /// allocator must outlive the returned `Bench`, which is naturally
    /// the case when it's held in `main`'s stack frame.
    pub fn new(bus: &'a UsbBusAllocator<B>, timer: Timer0, cfg: BenchConfig) -> Self {
        let serial = SerialPort::new(bus);
        let dev = UsbDeviceBuilder::new(bus, UsbVidPid(0x16c0, 0x27dd))
            .strings(&[StringDescriptors::default()
                .manufacturer(cfg.manufacturer)
                .product(cfg.product)
                .serial_number(cfg.serial)])
            .expect("USB strings")
            .max_packet_size_0(64)
            .expect("USB max packet size")
            .device_class(2)
            .build();
        Self { dev, serial, timer }
    }

    /// Pump the USB stack for roughly `us` microseconds. Call once at
    /// boot with `~2_000_000` so `cat /dev/ttyACM0` can attach before
    /// the first real line is written; without this the boot banner
    /// gets NAK'd and clobbered by the next line.
    pub fn enumerate_for(&mut self, us: u64) {
        let deadline = self.timer.get_counter().ticks().saturating_add(us);
        while self.timer.get_counter().ticks() < deadline {
            self.dev.poll(&mut [&mut self.serial]);
        }
    }

    /// Write `data` over the CDC serial port, polling the USB stack on
    /// every retry. Bounded by a 1 s deadline so a detached host can't
    /// wedge the firmware. Drains the TX FIFO for a further 20 ms
    /// before returning.
    pub fn write_line(&mut self, data: &[u8]) {
        let mut remaining = data;
        let deadline = self.timer.get_counter().ticks().saturating_add(1_000_000);
        while !remaining.is_empty() && self.timer.get_counter().ticks() < deadline {
            self.dev.poll(&mut [&mut self.serial]);
            match self.serial.write(remaining) {
                Ok(n) if n > 0 => remaining = remaining.get(n..).unwrap_or_default(),
                _ => {}
            }
        }
        let flush = self.timer.get_counter().ticks().saturating_add(20_000);
        while self.timer.get_counter().ticks() < flush {
            self.dev.poll(&mut [&mut self.serial]);
        }
    }

    /// `[iter] tag` on one line, used to bracket a measured call so a
    /// mid-run hang can be localised to the last marker printed.
    pub fn print_marker(&mut self, iter: u32, tag: &[u8]) {
        let mut out: String<64> = String::new();
        let _ = write!(&mut out, "[{iter}] ");
        self.write_line(out.as_bytes());
        self.write_line(tag);
    }

    /// `[iter] label: cycles=N us=U ms=M`, the canonical per-iteration
    /// result line. Takes a full `u64` cycle count so the caller is
    /// responsible for handling the 32-bit DWT wrap via
    /// [`crate::measure_cycles`].
    pub fn print_result(&mut self, iter: u32, label: &str, cycles: u64, sys_hz: u64) {
        let us = cycles.saturating_mul(1_000_000) / sys_hz;
        let mut out: String<128> = String::new();
        let _ = writeln!(
            &mut out,
            "[{iter}] {label}: cycles={cycles} us={us} ms={}",
            us / 1000,
        );
        self.write_line(out.as_bytes());
    }

    /// Busy-poll the USB stack for `us` microseconds. Used at the end
    /// of the main loop to space out iterations so the serial output
    /// is readable.
    pub fn pace(&mut self, us: u64) {
        let deadline = self.timer.get_counter().ticks().saturating_add(us);
        while self.timer.get_counter().ticks() < deadline {
            self.dev.poll(&mut [&mut self.serial]);
        }
    }
}
