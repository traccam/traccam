
use embassy_time::Duration;
use embassy_time::Timer;
use embassy_nrf::gpiote::InputChannel;

pub async fn wait_for_press<T: embassy_nrf::gpiote::Channel>(btn: &mut InputChannel<'_>) {
	btn.wait_for_low().await;

	Timer::after(Duration::from_millis(50)).await;

	btn.wait_for_high().await;

	Timer::after(Duration::from_millis(50)).await;
}