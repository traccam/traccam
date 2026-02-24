use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use std::thread::sleep;
use std::time::Duration;
use traccam_common::display::simulator::StateSimulator;

fn main() {
    let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(128, 32));

    let output_settings = OutputSettingsBuilder::new()
        .theme(BinaryColorTheme::OledWhite)
        .scale(6)
        .build();
    let mut w = Window::new("SSD1306 Sim", &output_settings);
    let mut generator = StateSimulator::default();
    loop {
        let state = generator.gen_next();
        traccam_common::display::draw_status_display(&mut display, &state);
        w.update(&display);
        sleep(Duration::from_secs(1));
        if w.events().find(|&e| e == SimulatorEvent::Quit).is_some() {
            break;
        }
    }
}
