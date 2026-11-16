#![no_std]
#![no_main]

use core::fmt::Write as _;

use embassy_executor::Spawner;
use embassy_rp::{
    bind_interrupts, dma,
    gpio::{Level, Output},
    peripherals::DMA_CH0,
    spi::{self, Spi},
};
use embassy_time::{Delay, Duration, Instant, Ticker};
use embedded_graphics::{
    framebuffer::{Framebuffer, buffer_size},
    pixelcolor::{
        Rgb565,
        raw::{BigEndian, RawU16},
    },
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};
use embedded_hal_bus::spi::ExclusiveDevice;
use font_shinonome::SHINONOME_12;
use heapless::String;
use panic_halt as _;
use ssd1351::{Panel, Rotation, Ssd1351};
use static_cell::ConstStaticCell;
use textdraw::TextStyle;

#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"oled"),
    embassy_rp::binary_info::rp_program_description!(c"SSD1351 drawing test for Tiny 2350"),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

bind_interrupts!(struct Irqs {
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>;
});

const SPI_FREQUENCY: u32 = 15_000_000;
const PANEL: Panel = Panel::Oled128x96;
const ROTATION: Rotation = Rotation::Deg0;

const WIDTH: usize = 128;
const HEIGHT: usize = 96;
const LINE_HEIGHT: i32 = 12;
const STATUS_TOP: i32 = 58;
const STATUS_ROWS: core::ops::Range<u16> = 58..82;

type FrameBuffer =
    Framebuffer<Rgb565, RawU16, BigEndian, WIDTH, HEIGHT, { buffer_size::<Rgb565>(WIDTH, HEIGHT) }>;

static FRAME: ConstStaticCell<FrameBuffer> = ConstStaticCell::new(FrameBuffer::new());

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut heartbeat = Output::new(p.PIN_19, Level::High);

    let mut config = spi::Config::default();
    config.frequency = SPI_FREQUENCY;
    let bus = Spi::new_txonly(p.SPI0, p.PIN_6, p.PIN_7, p.DMA_CH0, Irqs, config);
    let cs = Output::new(p.PIN_5, Level::High);
    let dc = Output::new(p.PIN_4, Level::Low);
    let rst = Output::new(p.PIN_3, Level::High);
    let device = ExclusiveDevice::new(bus, cs, Delay).unwrap();

    let mut display = Ssd1351::new(device, dc, rst, PANEL, ROTATION);
    display.init(&mut Delay).await.unwrap();

    let frame = FRAME.take();
    draw_static(frame);
    let started = Instant::now();
    display.flush(frame.data()).await.unwrap();
    let full_flush = started.elapsed();

    let mut partial_flush = Duration::from_ticks(0);
    let mut ticker = Ticker::every(Duration::from_secs(1));
    let boot = Instant::now();
    loop {
        draw_status(frame, boot.elapsed().as_secs(), full_flush, partial_flush);
        let started = Instant::now();
        display.flush_rows(frame.data(), STATUS_ROWS).await.unwrap();
        partial_flush = started.elapsed();

        heartbeat.toggle();
        ticker.next().await;
    }
}

fn draw_static(frame: &mut FrameBuffer) {
    let white = TextStyle::new(SHINONOME_12, Rgb565::WHITE);
    let yellow = TextStyle::new(SHINONOME_12, Rgb565::YELLOW);
    let fill = PrimitiveStyle::with_fill;

    frame.clear(Rgb565::BLACK).unwrap();
    Rectangle::new(Point::zero(), Size::new(WIDTH as u32, 14))
        .into_styled(fill(Rgb565::new(0, 16, 20)))
        .draw(frame)
        .unwrap();
    Circle::new(Point::new(84, 18), 40)
        .into_styled(fill(Rgb565::new(10, 0, 10)))
        .draw(frame)
        .unwrap();

    let lines = [
        (1, "SSD1351 表示テスト", white),
        (16, "漢字・かな・カナ", white),
        (30, "半角ｶﾅ ABC abc 123", white),
        (44, "145.800MHz ±5°", yellow),
    ];
    for (top, text, style) in lines {
        Text::with_baseline(text, Point::new(2, top), style, Baseline::Top)
            .draw(frame)
            .unwrap();
    }

    let bars = [
        Rgb565::RED,
        Rgb565::YELLOW,
        Rgb565::GREEN,
        Rgb565::CYAN,
        Rgb565::BLUE,
        Rgb565::MAGENTA,
        Rgb565::WHITE,
        Rgb565::new(8, 16, 8),
    ];
    let bar_width = WIDTH as u32 / bars.len() as u32;
    for (index, color) in bars.into_iter().enumerate() {
        Rectangle::new(
            Point::new(index as i32 * bar_width as i32, 84),
            Size::new(bar_width, HEIGHT as u32 - 84),
        )
        .into_styled(fill(color))
        .draw(frame)
        .unwrap();
    }
}

fn draw_status(frame: &mut FrameBuffer, seconds: u64, full: Duration, partial: Duration) {
    let style = TextStyle::new(SHINONOME_12, Rgb565::GREEN);
    let band = Size::new(WIDTH as u32, u32::from(STATUS_ROWS.end - STATUS_ROWS.start));
    Rectangle::new(Point::new(0, STATUS_TOP), band)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(frame)
        .unwrap();

    let mut elapsed: String<32> = String::new();
    write!(elapsed, "経過 {seconds} 秒").unwrap();
    let mut timing: String<32> = String::new();
    write!(
        timing,
        "全{}ms 部分{}us",
        full.as_millis(),
        partial.as_micros()
    )
    .unwrap();

    for (index, text) in [elapsed, timing].iter().enumerate() {
        let top = STATUS_TOP + index as i32 * LINE_HEIGHT;
        Text::with_baseline(text, Point::new(2, top), style, Baseline::Top)
            .draw(frame)
            .unwrap();
    }
}
