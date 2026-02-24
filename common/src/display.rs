use chrono::{FixedOffset, Timelike};
use core::fmt::Debug;
use embedded_graphics::Drawable;
use embedded_graphics::image::{Image, ImageRaw};
use embedded_graphics::mono_font::{MonoFont};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::mono_font::ascii::*;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::DrawTarget;
use embedded_graphics::prelude::Point;
use embedded_graphics::text::Baseline;
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