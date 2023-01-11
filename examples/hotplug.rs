use system76_power::hotplug;

fn main() -> hotplug::Result<()> {
    let nvidia_device_id = std::fs::read_to_string("/sys/bus/pci/devices/0000:01:00.0/device").ok();

    let mut emitter = hotplug::Emitter::new(nvidia_device_id);

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));

        for id in emitter.emit_on_detect() {
            println!("HotPlugDetect: {id}");
        }

        emitter.mux_step();
    }
}
