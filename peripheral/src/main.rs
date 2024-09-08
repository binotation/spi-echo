//! Echo received byte over SPI1 back to master. Only works when compiled in release mode.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use heapless::spsc::Queue;
use panic_semihosting as _; // logs messages to the host stderr; requires a debugger
use stm32u5::stm32u575::{interrupt, Interrupt, Peripherals, SPI1};

static mut SPI1_PERIPHERAL: Option<SPI1> = None;
static mut BUFFER: Option<Queue<u16, 16>> = None;

#[interrupt]
fn SPI1() {
    let spi1 = unsafe { SPI1_PERIPHERAL.as_mut() }.unwrap();
    let buffer = unsafe { BUFFER.as_mut() }.unwrap();

    if spi1.spi_sr().read().rxp().bit_is_set() {
        let received_byte = spi1.spi_rxdr().read().rxdr().bits() as u16;

        if buffer.enqueue(received_byte).is_ok() {
            spi1.spi_ier().modify(|_, w| w.txpie().set_bit());
        }
    }

    // No synchronization. I assume the reason this works is because the SPI clock rate is just right
    // such that the slave doesn't write too fast to cause an underrun.
    if spi1.spi_sr().read().txp().bit_is_set() {
        match buffer.dequeue() {
            Some(byte) => {
                spi1.txdr8().write(|w| unsafe { w.txdr().bits(byte as u8) });
                if buffer.is_empty() {
                    spi1.spi_ier().modify(|_, w| w.txpie().clear_bit());
                }
            }
            None => {
                spi1.spi_ier().modify(|_, w| w.txpie().clear_bit());
            }
        }
    }

    // Reset underrun error
    if spi1.spi_sr().read().udr().bit_is_set() {
        spi1.spi_ifcr().write(|w| w.udrc().set_bit());
    }
}

#[entry]
fn main() -> ! {
    // Device defaults to 4MHz clock

    let dp = Peripherals::take().unwrap();

    // Enable peripheral clocks - GPIOA, SPI
    dp.RCC.ahb2enr1().write(|w| w.gpioaen().enabled());
    dp.RCC.apb2enr().write(|w| w.spi1en().enabled());

    // SPI1: A4 (NSS), A5 (SCK), A6 (MISO), A7 (MOSI) as AF 5
    dp.GPIOA.moder().write(|w| {
        w.mode4()
            .alternate()
            .mode5()
            .alternate()
            .mode6()
            .alternate()
            .mode7()
            .alternate()
    });
    dp.GPIOA.ospeedr().write(|w| {
        w.ospeed4()
            .very_high_speed()
            .ospeed5()
            .very_high_speed()
            .ospeed6()
            .very_high_speed()
            .ospeed7()
            .very_high_speed()
    });
    dp.GPIOA.afrl().write(|w| {
        w.afsel4()
            .af5()
            .afsel5()
            .af5()
            .afsel6()
            .af5()
            .afsel7()
            .af5()
    });

    // Set data frame size to 8 bits
    dp.SPI1.spi_cfg1().write(|w| unsafe { w.dsize().bits(7) });
    // Set underrun dummy byte as '?'
    dp.SPI1
        .spi_udrdr()
        .write(|w| unsafe { w.udrdr().bits(b'?' as u32) });
    // Enable receive packet interrupt
    dp.SPI1.spi_ier().write(|w| w.rxpie().set_bit());
    // Enable SPI as slave
    dp.SPI1.spi_cr1().write(|w| w.spe().set_bit());
    // Load TX FIFO with initial byte '!'
    dp.SPI1.txdr8().write(|w| unsafe { w.txdr().bits(b'!') });

    unsafe {
        BUFFER = Some(Queue::default());
        // Unmask global interrupts
        cortex_m::peripheral::NVIC::unmask(Interrupt::SPI1);
        SPI1_PERIPHERAL = Some(dp.SPI1);
    }

    #[allow(clippy::empty_loop)]
    loop {}
}
