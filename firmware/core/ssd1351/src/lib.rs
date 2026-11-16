#![cfg_attr(not(test), no_std)]

use core::ops::Range;

use embedded_hal::digital::{ErrorType as PinErrorType, OutputPin};
use embedded_hal_async::{
    delay::DelayNs,
    spi::{ErrorType as SpiErrorType, SpiDevice},
};

const SET_COLUMN: u8 = 0x15;
const SET_ROW: u8 = 0x75;
const WRITE_RAM: u8 = 0x5C;
const SET_REMAP: u8 = 0xA0;
const SET_START_LINE: u8 = 0xA1;
const SET_DISPLAY_OFFSET: u8 = 0xA2;
const NORMAL_DISPLAY: u8 = 0xA6;
const FUNCTION_SELECT: u8 = 0xAB;
const DISPLAY_OFF: u8 = 0xAE;
const DISPLAY_ON: u8 = 0xAF;
const SET_PHASE_LENGTH: u8 = 0xB1;
const SET_CLOCK_DIV: u8 = 0xB3;
const SET_VSL: u8 = 0xB4;
const SET_GPIO: u8 = 0xB5;
const SET_SECOND_PRECHARGE: u8 = 0xB6;
const SET_VCOMH: u8 = 0xBE;
const SET_CONTRAST_ABC: u8 = 0xC1;
const SET_MASTER_CONTRAST: u8 = 0xC7;
const SET_MUX_RATIO: u8 = 0xCA;
const SET_COMMAND_LOCK: u8 = 0xFD;

const REMAP_BASE: u8 = 0b0010_0100;
const REMAP_VERTICAL_INCREMENT: u8 = 0b0000_0001;
const REMAP_COLUMN_REVERSE: u8 = 0b0000_0010;
const REMAP_COM_REVERSE: u8 = 0b0001_0000;

pub const WIDTH: u16 = 128;
pub const BYTES_PER_PIXEL: usize = 2;
pub const MAX_CONTRAST: u8 = 15;

const ROW_LEN: usize = WIDTH as usize * BYTES_PER_PIXEL;
const BLANK_ROW: [u8; ROW_LEN] = [0; ROW_LEN];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Oled128x96,
    Oled128x128,
}

impl Panel {
    pub const fn height(self) -> u16 {
        match self {
            Panel::Oled128x96 => 96,
            Panel::Oled128x128 => 128,
        }
    }

    pub const fn frame_len(self) -> usize {
        ROW_LEN * self.height() as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

impl Rotation {
    const fn remap(self) -> u8 {
        REMAP_BASE
            | match self {
                Rotation::Deg0 => REMAP_COM_REVERSE,
                Rotation::Deg90 => {
                    REMAP_COM_REVERSE | REMAP_COLUMN_REVERSE | REMAP_VERTICAL_INCREMENT
                }
                Rotation::Deg180 => REMAP_COLUMN_REVERSE,
                Rotation::Deg270 => REMAP_VERTICAL_INCREMENT,
            }
    }

    const fn is_vertical(self) -> bool {
        matches!(self, Rotation::Deg90 | Rotation::Deg270)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<SpiE, PinE> {
    Spi(SpiE),
    Pin(PinE),
    OutOfRange,
    LengthMismatch { expected: usize, actual: usize },
}

type DriverResult<SPI, DC> =
    Result<(), Error<<SPI as SpiErrorType>::Error, <DC as PinErrorType>::Error>>;

pub struct Ssd1351<SPI, DC, RST> {
    spi: SPI,
    dc: DC,
    rst: RST,
    panel: Panel,
    rotation: Rotation,
}

impl<SPI, DC, RST> Ssd1351<SPI, DC, RST>
where
    SPI: SpiDevice,
    DC: OutputPin,
    RST: OutputPin<Error = DC::Error>,
{
    pub const fn new(spi: SPI, dc: DC, rst: RST, panel: Panel, rotation: Rotation) -> Self {
        Self {
            spi,
            dc,
            rst,
            panel,
            rotation,
        }
    }

    pub fn release(self) -> (SPI, DC, RST) {
        (self.spi, self.dc, self.rst)
    }

    pub const fn panel(&self) -> Panel {
        self.panel
    }

    pub const fn rotation(&self) -> Rotation {
        self.rotation
    }

    pub const fn size(&self) -> (u16, u16) {
        let height = self.panel.height();
        if self.rotation.is_vertical() {
            (height, WIDTH)
        } else {
            (WIDTH, height)
        }
    }

    pub async fn init(&mut self, delay: &mut impl DelayNs) -> DriverResult<SPI, DC> {
        self.rst.set_high().map_err(Error::Pin)?;
        delay.delay_ms(1).await;
        self.rst.set_low().map_err(Error::Pin)?;
        delay.delay_ms(10).await;
        self.rst.set_high().map_err(Error::Pin)?;
        delay.delay_ms(1).await;

        let mux_ratio = (self.panel.height() - 1) as u8;
        self.command(SET_COMMAND_LOCK, &[0x12]).await?;
        self.command(SET_COMMAND_LOCK, &[0xB1]).await?;
        self.command(DISPLAY_OFF, &[]).await?;
        self.command(SET_CLOCK_DIV, &[0xF1]).await?;
        self.command(SET_MUX_RATIO, &[mux_ratio]).await?;
        self.command(SET_DISPLAY_OFFSET, &[0x00]).await?;
        self.command(SET_START_LINE, &[0x00]).await?;
        self.command(SET_GPIO, &[0x00]).await?;
        self.command(FUNCTION_SELECT, &[0x01]).await?;
        self.command(SET_VSL, &[0xA0, 0xB5, 0x55]).await?;
        self.command(SET_CONTRAST_ABC, &[0xC8, 0x8F, 0xC8]).await?;
        self.command(SET_MASTER_CONTRAST, &[MAX_CONTRAST]).await?;
        self.command(SET_PHASE_LENGTH, &[0x32]).await?;
        self.command(SET_SECOND_PRECHARGE, &[0x01]).await?;
        self.command(SET_VCOMH, &[0x05]).await?;
        self.command(NORMAL_DISPLAY, &[]).await?;
        self.command(SET_REMAP, &[self.rotation.remap()]).await?;
        self.clear().await?;
        self.command(DISPLAY_ON, &[]).await
    }

    pub async fn set_rotation(&mut self, rotation: Rotation) -> DriverResult<SPI, DC> {
        self.command(SET_REMAP, &[rotation.remap()]).await?;
        self.rotation = rotation;
        Ok(())
    }

    pub async fn set_display_on(&mut self, on: bool) -> DriverResult<SPI, DC> {
        self.command(if on { DISPLAY_ON } else { DISPLAY_OFF }, &[])
            .await
    }

    pub async fn set_contrast(&mut self, level: u8) -> DriverResult<SPI, DC> {
        if level > MAX_CONTRAST {
            return Err(Error::OutOfRange);
        }
        self.command(SET_MASTER_CONTRAST, &[level]).await
    }

    pub async fn flush(&mut self, frame: &[u8]) -> DriverResult<SPI, DC> {
        let (width, height) = self.size();
        self.write_area(0, 0, width, height, frame).await
    }

    pub async fn flush_rows(&mut self, frame: &[u8], rows: Range<u16>) -> DriverResult<SPI, DC> {
        check_len(self.panel.frame_len(), frame.len())?;
        let (width, height) = self.size();
        if rows.is_empty() || rows.end > height {
            return Err(Error::OutOfRange);
        }

        let row_len = usize::from(width) * BYTES_PER_PIXEL;
        let pixels = &frame[usize::from(rows.start) * row_len..usize::from(rows.end) * row_len];
        self.write_area(0, rows.start, width, rows.end - rows.start, pixels)
            .await
    }

    pub async fn write_area(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        pixels: &[u8],
    ) -> DriverResult<SPI, DC> {
        let (display_width, display_height) = self.size();
        let fits = |start: u16, len: u16, limit: u16| {
            len > 0 && start.checked_add(len).is_some_and(|end| end <= limit)
        };
        if !fits(x, width, display_width) || !fits(y, height, display_height) {
            return Err(Error::OutOfRange);
        }
        check_len(
            usize::from(width) * usize::from(height) * BYTES_PER_PIXEL,
            pixels.len(),
        )?;

        self.set_window(x, y, width, height).await?;
        self.command(WRITE_RAM, pixels).await
    }

    async fn clear(&mut self) -> DriverResult<SPI, DC> {
        let (width, height) = self.size();
        self.set_window(0, 0, width, height).await?;
        self.command(WRITE_RAM, &[]).await?;
        for _ in 0..self.panel.height() {
            self.data(&BLANK_ROW).await?;
        }
        Ok(())
    }

    async fn set_window(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> DriverResult<SPI, DC> {
        let mut columns = [x as u8, (x + width - 1) as u8];
        let mut rows = [y as u8, (y + height - 1) as u8];
        if self.rotation.is_vertical() {
            core::mem::swap(&mut columns, &mut rows);
        }
        self.command(SET_COLUMN, &columns).await?;
        self.command(SET_ROW, &rows).await
    }

    async fn command(&mut self, command: u8, arguments: &[u8]) -> DriverResult<SPI, DC> {
        self.dc.set_low().map_err(Error::Pin)?;
        self.spi.write(&[command]).await.map_err(Error::Spi)?;
        if !arguments.is_empty() {
            self.data(arguments).await?;
        }
        Ok(())
    }

    async fn data(&mut self, bytes: &[u8]) -> DriverResult<SPI, DC> {
        self.dc.set_high().map_err(Error::Pin)?;
        self.spi.write(bytes).await.map_err(Error::Spi)
    }
}

fn check_len<SpiE, PinE>(expected: usize, actual: usize) -> Result<(), Error<SpiE, PinE>> {
    if expected == actual {
        Ok(())
    } else {
        Err(Error::LengthMismatch { expected, actual })
    }
}

#[cfg(test)]
mod tests {
    use core::{
        convert::Infallible,
        pin::pin,
        task::{Context, Poll, Waker},
    };
    use std::{cell::RefCell, rc::Rc};

    use embedded_hal_async::spi::Operation;

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Event {
        Reset(bool),
        DelayNs(u32),
        Command(u8),
        Data(Vec<u8>),
    }

    #[derive(Default)]
    struct Bus {
        dc_high: bool,
        events: Vec<Event>,
    }

    type Shared = Rc<RefCell<Bus>>;

    struct FakeSpi(Shared);
    struct FakeDc(Shared);
    struct FakeRst(Shared);
    struct FakeDelay(Shared);

    impl SpiErrorType for FakeSpi {
        type Error = Infallible;
    }

    impl SpiDevice for FakeSpi {
        async fn transaction(
            &mut self,
            operations: &mut [Operation<'_, u8>],
        ) -> Result<(), Infallible> {
            let bus = &mut *self.0.borrow_mut();
            for operation in operations {
                let Operation::Write(bytes) = operation else {
                    panic!("unexpected SPI operation {operation:?}");
                };
                if !bus.dc_high {
                    assert_eq!(bytes.len(), 1, "commands are sent one byte at a time");
                    bus.events.push(Event::Command(bytes[0]));
                } else if let Some(Event::Data(data)) = bus.events.last_mut() {
                    data.extend_from_slice(bytes);
                } else {
                    bus.events.push(Event::Data(bytes.to_vec()));
                }
            }
            Ok(())
        }
    }

    impl PinErrorType for FakeDc {
        type Error = Infallible;
    }

    impl OutputPin for FakeDc {
        fn set_low(&mut self) -> Result<(), Infallible> {
            self.0.borrow_mut().dc_high = false;
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Infallible> {
            self.0.borrow_mut().dc_high = true;
            Ok(())
        }
    }

    impl PinErrorType for FakeRst {
        type Error = Infallible;
    }

    impl OutputPin for FakeRst {
        fn set_low(&mut self) -> Result<(), Infallible> {
            self.0.borrow_mut().events.push(Event::Reset(false));
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Infallible> {
            self.0.borrow_mut().events.push(Event::Reset(true));
            Ok(())
        }
    }

    impl DelayNs for FakeDelay {
        async fn delay_ns(&mut self, ns: u32) {
            self.0.borrow_mut().events.push(Event::DelayNs(ns));
        }
    }

    type TestDisplay = Ssd1351<FakeSpi, FakeDc, FakeRst>;
    type TestError = Error<Infallible, Infallible>;

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("the fake bus never blocks"),
        }
    }

    fn display(panel: Panel, rotation: Rotation) -> (Shared, TestDisplay) {
        let bus = Shared::default();
        let display = Ssd1351::new(
            FakeSpi(bus.clone()),
            FakeDc(bus.clone()),
            FakeRst(bus.clone()),
            panel,
            rotation,
        );
        (bus, display)
    }

    fn take_events(bus: &Shared) -> Vec<Event> {
        core::mem::take(&mut bus.borrow_mut().events)
    }

    fn command(code: u8, arguments: &[u8]) -> Vec<Event> {
        let mut events = vec![Event::Command(code)];
        if !arguments.is_empty() {
            events.push(Event::Data(arguments.to_vec()));
        }
        events
    }

    fn row_numbered_frame(panel: Panel) -> Vec<u8> {
        (0..panel.height())
            .flat_map(|row| [row as u8; ROW_LEN])
            .collect()
    }

    #[test]
    fn init_resets_and_configures_a_128x96_panel() {
        let (bus, mut display) = display(Panel::Oled128x96, Rotation::Deg0);

        block_on(display.init(&mut FakeDelay(bus.clone()))).unwrap();

        let mut expected = vec![
            Event::Reset(true),
            Event::DelayNs(1_000_000),
            Event::Reset(false),
            Event::DelayNs(10_000_000),
            Event::Reset(true),
            Event::DelayNs(1_000_000),
        ];
        for (code, arguments) in [
            (0xFD, &[0x12][..]),
            (0xFD, &[0xB1]),
            (0xAE, &[]),
            (0xB3, &[0xF1]),
            (0xCA, &[95]),
            (0xA2, &[0x00]),
            (0xA1, &[0x00]),
            (0xB5, &[0x00]),
            (0xAB, &[0x01]),
            (0xB4, &[0xA0, 0xB5, 0x55]),
            (0xC1, &[0xC8, 0x8F, 0xC8]),
            (0xC7, &[0x0F]),
            (0xB1, &[0x32]),
            (0xB6, &[0x01]),
            (0xBE, &[0x05]),
            (0xA6, &[]),
            (0xA0, &[0x34]),
            (0x15, &[0, 127]),
            (0x75, &[0, 95]),
            (0x5C, &[0; 128 * 96 * 2]),
            (0xAF, &[]),
        ] {
            expected.extend(command(code, arguments));
        }
        assert_eq!(take_events(&bus), expected);
    }

    #[test]
    fn init_drives_every_row_of_a_128x128_panel() {
        let (bus, mut display) = display(Panel::Oled128x128, Rotation::Deg0);

        block_on(display.init(&mut FakeDelay(bus.clone()))).unwrap();

        let events = take_events(&bus);
        let mux = events
            .iter()
            .position(|event| *event == Event::Command(0xCA))
            .unwrap();
        assert_eq!(events[mux + 1], Event::Data(vec![127]));
        let write = events
            .iter()
            .position(|event| *event == Event::Command(0x5C))
            .unwrap();
        assert_eq!(events[write - 1], Event::Data(vec![0, 127]));
        assert_eq!(events[write + 1], Event::Data(vec![0; 128 * 128 * 2]));
    }

    #[test]
    fn rotation_selects_the_remap_mode_and_logical_size() {
        let (bus, mut display) = display(Panel::Oled128x96, Rotation::Deg0);

        for (rotation, remap, size) in [
            (Rotation::Deg90, 0x37, (96, 128)),
            (Rotation::Deg180, 0x26, (128, 96)),
            (Rotation::Deg270, 0x25, (96, 128)),
            (Rotation::Deg0, 0x34, (128, 96)),
        ] {
            block_on(display.set_rotation(rotation)).unwrap();
            assert_eq!(take_events(&bus), command(0xA0, &[remap]), "{rotation:?}");
            assert_eq!(display.size(), size, "{rotation:?}");
            assert_eq!(display.rotation(), rotation);
        }
    }

    #[test]
    fn write_area_sets_the_window_before_writing_ram() {
        let (bus, mut display) = display(Panel::Oled128x96, Rotation::Deg0);

        block_on(display.write_area(10, 20, 2, 3, &[0xAB; 12])).unwrap();

        let mut expected = command(0x15, &[10, 11]);
        expected.extend(command(0x75, &[20, 22]));
        expected.extend(command(0x5C, &[0xAB; 12]));
        assert_eq!(take_events(&bus), expected);
    }

    #[test]
    fn vertical_rotations_swap_window_columns_and_rows() {
        let (bus, mut display) = display(Panel::Oled128x96, Rotation::Deg90);

        block_on(display.write_area(10, 20, 2, 3, &[0xAB; 12])).unwrap();

        let mut expected = command(0x15, &[20, 22]);
        expected.extend(command(0x75, &[10, 11]));
        expected.extend(command(0x5C, &[0xAB; 12]));
        assert_eq!(take_events(&bus), expected);
    }

    #[test]
    fn flush_sends_the_whole_frame_in_one_window() {
        let (bus, mut display) = display(Panel::Oled128x96, Rotation::Deg270);
        let frame = row_numbered_frame(Panel::Oled128x96);

        block_on(display.flush(&frame)).unwrap();

        let mut expected = command(0x15, &[0, 127]);
        expected.extend(command(0x75, &[0, 95]));
        expected.extend(command(0x5C, &frame));
        assert_eq!(take_events(&bus), expected);
    }

    #[test]
    fn flush_rows_sends_only_the_selected_rows() {
        let (bus, mut display) = display(Panel::Oled128x96, Rotation::Deg0);
        let frame = row_numbered_frame(Panel::Oled128x96);

        block_on(display.flush_rows(&frame, 10..12)).unwrap();

        let mut pixels = vec![10; ROW_LEN];
        pixels.extend([11; ROW_LEN]);
        let mut expected = command(0x15, &[0, 127]);
        expected.extend(command(0x75, &[10, 11]));
        expected.extend(command(0x5C, &pixels));
        assert_eq!(take_events(&bus), expected);
    }

    #[test]
    fn display_control_commands() {
        let (bus, mut display) = display(Panel::Oled128x96, Rotation::Deg0);

        block_on(display.set_contrast(7)).unwrap();
        block_on(display.set_display_on(false)).unwrap();
        block_on(display.set_display_on(true)).unwrap();

        let mut expected = command(0xC7, &[7]);
        expected.extend(command(0xAE, &[]));
        expected.extend(command(0xAF, &[]));
        assert_eq!(take_events(&bus), expected);
    }

    #[test]
    fn invalid_arguments_are_rejected_without_bus_traffic() {
        let (bus, mut display) = display(Panel::Oled128x96, Rotation::Deg0);
        let frame = row_numbered_frame(Panel::Oled128x96);
        let length_mismatch =
            |expected, actual| Err(TestError::LengthMismatch { expected, actual });

        assert_eq!(
            block_on(display.write_area(127, 0, 2, 1, &[0; 4])),
            Err(Error::OutOfRange)
        );
        assert_eq!(
            block_on(display.write_area(0, 95, 1, 2, &[0; 4])),
            Err(Error::OutOfRange)
        );
        assert_eq!(
            block_on(display.write_area(0, 0, 0, 1, &[])),
            Err(Error::OutOfRange)
        );
        assert_eq!(
            block_on(display.write_area(u16::MAX, 0, 2, 1, &[0; 4])),
            Err(Error::OutOfRange)
        );
        assert_eq!(
            block_on(display.write_area(0, 0, 2, 2, &[0; 7])),
            length_mismatch(8, 7)
        );
        assert_eq!(
            block_on(display.flush(&frame[1..])),
            length_mismatch(24576, 24575)
        );
        assert_eq!(
            block_on(display.flush_rows(&frame, 95..97)),
            Err(Error::OutOfRange)
        );
        assert_eq!(
            block_on(display.flush_rows(&frame, 5..5)),
            Err(Error::OutOfRange)
        );
        assert_eq!(
            block_on(display.flush_rows(&frame[1..], 0..1)),
            length_mismatch(24576, 24575)
        );
        assert_eq!(block_on(display.set_contrast(16)), Err(Error::OutOfRange));
        assert_eq!(take_events(&bus), []);
    }
}
