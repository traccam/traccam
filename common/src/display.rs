use chrono::{FixedOffset, Timelike};
use core::fmt::Debug;
use std::cmp::{max, min};
use embedded_graphics::Drawable;
use embedded_graphics::geometry::Size;
use embedded_graphics::image::{Image, ImageRaw};
use embedded_graphics::mono_font::{MonoFont};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::mono_font::ascii::*;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::{DrawTarget, Primitive};
use embedded_graphics::prelude::Point;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Alignment, Baseline, TextStyleBuilder};
use embedded_graphics::text::Text;
use crate::{DisplayState, DisplayTZ};

pub struct Warnings {
    no_pps: bool,
}

pub fn draw_status_display<D>(display: &mut D, state: &DisplayState)
where
    D: DrawTarget<Color = BinaryColor>,
    D::Error: Debug,
{
    const GPS_FONT: MonoFont = FONT_6X13;
    const TXT_B: MonoTextStyleBuilder<BinaryColor> = MonoTextStyleBuilder::new().font(&GPS_FONT);
    const TXT: MonoTextStyleBuilder<BinaryColor> = TXT_B.text_color(BinaryColor::On);
    const TXT_INV: MonoTextStyleBuilder<BinaryColor> = TXT_B.text_color(BinaryColor::Off).background_color(BinaryColor::On);
    const fn yoffs(i: u8) -> i32 {
        let neg_margin = 2;
        -neg_margin + (GPS_FONT.character_size.height as i32 - neg_margin) * i as i32
    }

    display.clear(BinaryColor::Off).unwrap();
    let blink = state.time.second() % 2 == 1;

    // Lat
    Text::with_baseline(
        &heapless::format!(10; "N{:>8.5}", state.lat).unwrap(),
        Point::new(0, yoffs(0)),
        TXT.build(),
        Baseline::Top,
    )
    .draw(display)
    .unwrap();

    // Lon
    Text::with_baseline(
        &heapless::format!(10; "E{:>8.5}", state.lon).unwrap(),
        Point::new(0, yoffs(1)),
        TXT.build(),
        Baseline::Top,
    )
    .draw(display)
    .unwrap();

    let now = if let DisplayTZ::Local {fixed_offset} = state.display_tz {
        state.now().with_timezone(&fixed_offset)
    } else {
        state.now().with_timezone(&FixedOffset::east_opt(0).expect("Infallible. UTC."))
    };
    // Clock
    Text::with_baseline(&heapless::format!(30; " {:02}:{:02}:{:02}", now.hour(), now.minute(), now.second()).unwrap(), Point::new(0, yoffs(2)), TXT.build(), Baseline::Top)
        .draw(display)
        .unwrap();

    Image::new(&if state.display_tz == DisplayTZ::Utc {UTC_90DEG } else { LOC_90DEG }, Point::new(0, 21)).draw(display).unwrap();
    draw_16_16("NO", "FIX", Point::new(54,0), blink, display);
    draw_16_16("BAD", "FIX", Point::new(54,16), blink, display);
    draw_16_16("NO", "PPS", Point::new(54 + 16,0), blink, display);
    draw_16_16("BAD", "PPS", Point::new(54 + 16,16), blink, display);
}

pub fn draw_16_16<D>(l1: &str, l2: &str, top_left: Point, blink: bool, display: &mut D)
where
    D: DrawTarget<Color = BinaryColor>,
    D::Error: Debug,
{
    Rectangle::new(top_left, Size::new(16, 16))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(display)
        .unwrap();

    if blink {
        Rectangle::new(top_left + Point::new(1,1), Size::new(14, 14))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
            .draw(display)
            .unwrap();
    }

    let center_point = top_left + Point::new(8, 1);

    let text_style = TextStyleBuilder::new()
        .alignment(Alignment::Center)
        .baseline(Baseline::Top)
        .build();

    Text::with_text_style(
        &heapless::format!(7; "{}\n{}", &l1[..min(l1.len(), 3)], &l2[..min(l2.len(), 3)]).unwrap(),
        center_point,
        MonoTextStyleBuilder::new().font(&FONT_5X7).text_color(if blink { BinaryColor::On } else { BinaryColor::Off }).build(),
        text_style,
    )
        .draw(display)
        .unwrap();
}

#[cfg(feature = "simulated_data")]
pub mod simulator {
    use crate::{DisplayState, DisplayTZ};
    use chrono::{Local};

    #[derive(Default)]
    pub struct StateSimulator {
        rng: fastrand::Rng,
    }

    impl StateSimulator {
        pub fn gen_next(&mut self) -> DisplayState {
            let now = Local::now().to_utc();
            DisplayState {
                time: now.time(),
                date: now.date_naive(),
                display_tz: DisplayTZ::Utc,
                lat: self.rng.f64() * 60.0,
                lon: self.rng.f64() * 60.0,
                sats: self.rng.u8(0..99),
                hdop: 99.0,
            }
        }
    }
}

pub const UTC_90DEG: ImageRaw<BinaryColor> = ImageRaw::new(&[
    0x88, // #...#... (C right)
    0x88, // #...#... (C middle)
    0xF8, // #####... (C left/back)
    0x00, // ........ (Spacing)
    0x80, // #....... (T right)
    0xF8, // #####... (T stem)
    0x80, // #....... (T left)
    0x00, // ........ (Spacing)
    0xF8, // #####... (U right)
    0x08, // ....#... (U bottom)
    0xF8, // #####... (U left)
], 6);

pub const LOC_90DEG: ImageRaw<BinaryColor> = ImageRaw::new(&[
    0x88, // #...#... (C right)
    0x88, // #...#... (C middle)
    0xF8, // #####... (C left/back)
    0x00, // ........ (Spacing)
    0xF8, // #####... (O right)
    0x88, // #...#... (O middle)
    0xF8, // #####... (O left)
    0x00, // ........ (Spacing)
    0x08, // ....#... (L right)
    0x08, // ....#... (L middle)
    0xF8, // #####... (L vertical stem)
], 5);