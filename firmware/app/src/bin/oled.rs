#![no_std]
#![no_main]

use core::{
    fmt::Write as _,
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};

use embassy_executor::Spawner;
use embassy_rp::{
    bind_interrupts, dma,
    gpio::{Input, Level, Output, Pull},
    peripherals::DMA_CH0,
    spi::{self, Spi},
};
use embassy_time::{Delay, Duration, Ticker, Timer};
use embedded_graphics::{
    framebuffer::{Framebuffer, buffer_size},
    pixelcolor::{
        Rgb565,
        raw::{BigEndian, RawU16},
    },
    prelude::*,
    primitives::Rectangle,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use embedded_hal_bus::spi::ExclusiveDevice;
use font_shinonome::SHINONOME_12;
use heapless::{Deque, String};
use panic_halt as _;
use ssd1351::{Panel, Rotation, Ssd1351};
use static_cell::ConstStaticCell;
use textdraw::{LineBreak, TextStyle};

#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"oled"),
    embassy_rp::binary_info::rp_program_description!(
        c"Scrolls sample texts on the SSD1351 for Tiny 2350; BOOT switches between them"
    ),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

bind_interrupts!(struct Irqs {
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>;
});

struct Work {
    title: &'static str,
    text: &'static str,
}

const WORKS: [Work; 4] = [
    Work {
        title: "坊っちゃん",
        text: include_str!(concat!(env!("OUT_DIR"), "/botchan.txt")),
    },
    Work {
        title: "世界人権宣言",
        text: include_str!(concat!(env!("OUT_DIR"), "/udhr.txt")),
    },
    Work {
        title: "ポラーノの広場",
        text: include_str!(concat!(env!("OUT_DIR"), "/polano.txt")),
    },
    Work {
        title: "Lorem ipsum",
        text: include_str!(concat!(env!("OUT_DIR"), "/lorem-ipsum.txt")),
    },
];

const SPI_FREQUENCY: u32 = 15_000_000;
const PANEL: Panel = Panel::Oled128x96;
const ROTATION: Rotation = Rotation::Deg0;
const FRAME_INTERVAL: Duration = Duration::from_millis(40);
const DEBOUNCE: Duration = Duration::from_millis(30);

const WIDTH: usize = 128;
const HEIGHT: usize = 96;
const HEADER_ROWS: Range<u16> = 0..14;
const BODY_ROWS: Range<u16> = 14..HEIGHT as u16;
const BODY_TOP: i32 = BODY_ROWS.start as i32;
const BODY_HEIGHT: i32 = (BODY_ROWS.end - BODY_ROWS.start) as i32;
const LINE_PITCH: i32 = 13;
const VISIBLE_LINES: usize = (BODY_HEIGHT as usize).div_ceil(LINE_PITCH as usize) + 1;
const TEXT_X: i32 = 4;
const TEXT_WIDTH: u32 = WIDTH as u32 - 2 * TEXT_X as u32;

const HEADER_COLOR: Rgb565 = Rgb565::new(0, 16, 20);
const BODY_COLOR: Rgb565 = Rgb565::new(28, 56, 26);

type FrameBuffer =
    Framebuffer<Rgb565, RawU16, BigEndian, WIDTH, HEIGHT, { buffer_size::<Rgb565>(WIDTH, HEIGHT) }>;

static FRAME: ConstStaticCell<FrameBuffer> = ConstStaticCell::new(FrameBuffer::new());
static SWITCH_REQUESTED: AtomicBool = AtomicBool::new(false);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut heartbeat = Output::new(p.PIN_19, Level::High);
    let boot_button = Input::new(p.PIN_23, Pull::Up);
    spawner.spawn(watch_boot_button(boot_button).unwrap());

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
    let mut work = 0;
    let mut scroller = Scroller::new(WORKS[work].text);
    let mut shown_progress = scroller.progress();
    draw_header(frame, WORKS[work].title, shown_progress);
    draw_body(frame, &scroller);
    display.flush(frame.data()).await.unwrap();

    let mut ticker = Ticker::every(FRAME_INTERVAL);
    loop {
        ticker.next().await;

        if SWITCH_REQUESTED.swap(false, Ordering::Relaxed) {
            work = (work + 1) % WORKS.len();
            scroller = Scroller::new(WORKS[work].text);
            shown_progress = scroller.progress();
            draw_header(frame, WORKS[work].title, shown_progress);
            draw_body(frame, &scroller);
            display.flush(frame.data()).await.unwrap();
            continue;
        }

        if scroller.advance() {
            heartbeat.toggle();
        }
        draw_body(frame, &scroller);
        display.flush_rows(frame.data(), BODY_ROWS).await.unwrap();

        let progress = scroller.progress();
        if progress != shown_progress {
            draw_header(frame, WORKS[work].title, progress);
            display.flush_rows(frame.data(), HEADER_ROWS).await.unwrap();
            shown_progress = progress;
        }
    }
}

#[embassy_executor::task]
async fn watch_boot_button(mut button: Input<'static>) {
    loop {
        button.wait_for_falling_edge().await;
        SWITCH_REQUESTED.store(true, Ordering::Relaxed);
        Timer::after(DEBOUNCE).await;
        button.wait_for_high().await;
        Timer::after(DEBOUNCE).await;
    }
}

struct Scroller {
    text: &'static str,
    lines: Deque<&'static str, VISIBLE_LINES>,
    cursor: usize,
    trailing_blanks: usize,
    offset: i32,
}

impl Scroller {
    fn new(text: &'static str) -> Self {
        let mut scroller = Self {
            text,
            lines: Deque::new(),
            cursor: 0,
            trailing_blanks: 0,
            offset: 0,
        };
        while !scroller.lines.is_full() {
            let line = scroller.next_line();
            let _ = scroller.lines.push_back(line);
        }
        scroller
    }

    fn advance(&mut self) -> bool {
        self.offset += 1;
        if self.offset < LINE_PITCH {
            return false;
        }
        self.offset = 0;
        self.lines.pop_front();
        let line = self.next_line();
        let _ = self.lines.push_back(line);
        true
    }

    fn next_line(&mut self) -> &'static str {
        if self.cursor >= self.text.len() {
            self.trailing_blanks += 1;
            if self.trailing_blanks >= VISIBLE_LINES {
                self.cursor = 0;
                self.trailing_blanks = 0;
            }
            return "";
        }
        let rest = &self.text[self.cursor..];
        let LineBreak { end, next } = SHINONOME_12.break_line(rest, TEXT_WIDTH);
        self.cursor += next;
        &rest[..end]
    }

    fn progress(&self) -> usize {
        self.cursor * 100 / self.text.len()
    }
}

fn draw_header(frame: &mut FrameBuffer, title: &str, progress: usize) {
    let area = Rectangle::new(
        Point::zero(),
        Size::new(WIDTH as u32, u32::from(HEADER_ROWS.end)),
    );
    let style = TextStyle::new(SHINONOME_12, Rgb565::WHITE);
    let right = TextStyleBuilder::new()
        .alignment(Alignment::Right)
        .baseline(Baseline::Top)
        .build();
    let mut label: String<8> = String::new();
    write!(label, "{progress}%").unwrap();

    frame.fill_solid(&area, HEADER_COLOR).unwrap();
    Text::with_baseline(title, Point::new(TEXT_X, 1), style, Baseline::Top)
        .draw(frame)
        .unwrap();
    Text::with_text_style(&label, Point::new(WIDTH as i32 - TEXT_X, 1), style, right)
        .draw(frame)
        .unwrap();
}

fn draw_body(frame: &mut FrameBuffer, scroller: &Scroller) {
    let area = Rectangle::new(
        Point::new(0, BODY_TOP),
        Size::new(WIDTH as u32, BODY_HEIGHT as u32),
    );
    let style = TextStyle::new(SHINONOME_12, BODY_COLOR);

    frame.fill_solid(&area, Rgb565::BLACK).unwrap();
    let mut body = frame.clipped(&area);
    for (index, line) in scroller.lines.iter().enumerate() {
        let top = BODY_TOP + 1 + index as i32 * LINE_PITCH - scroller.offset;
        Text::with_baseline(line, Point::new(TEXT_X, top), style, Baseline::Top)
            .draw(&mut body)
            .unwrap();
    }
}
