use system76_power::hid_backlight;

fn main() {
    env_logger::init();
    hid_backlight::daemon();
}
