#[tokio::main(flavor = "current_thread")]
async fn main() { system76_power::hid_backlight::daemon().await; }
