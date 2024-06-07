# System76 Power Management

**system76-power** is a utility for managing graphics and power profiles.

## Switchable Graphics

Switchable graphics is a feature for laptops and all-in-one PCs. It is not
supported on desktops.

A reboot is **required** for changes to take effect after switching modes.

### Integrated

The integrated graphics controller on the Intel or AMD CPU is used exclusively.

Lower graphical performance with a longer battery life.

External displays connected to the dGPU ports cannot be used.

### NVIDIA

The dGPU (NVIDIA) is used exclusively.

Higher graphical performance at the expense of a shorter battery life.

Allows using external displays.

### Hybrid

Enables PRIME render offloading. The iGPU is used as the primary renderer, with
the ability to have specific applications render using the dGPU.

PRIME render offloading requires the 435.17 NVIDIA drivers or later.

Applications must use [GLVND] to take advantage of this feature, so may not
render on the dGPU even when requested. Vulkan applications must be launched
with `__NV_PRIME_RENDER_OFFLOAD=1` to render on the dGPU. GLX applications must
be launched with `__NV_PRIME_RENDER_OFFLOAD=1 __GLX_VENDOR_LIBRARY_NAME=nvidia`
to render on the dGPU.

Display offload sinks ("reverse PRIME") require 450.57 NVIDIA drivers or later.
This feature allows using external displays while in this mode.

GPU support for run-time power management is required for the device to enter
a low power state when not used. Only Turing cards and newer fully implement
this functionality. Support for run-time power manage can be checked in the
`supported-gpus.json` file provided by the driver. e.g.:

```
$ cat /sys/bus/pci/devices/0000:01:00.0/device
0x1f15
$ jq '.chips[] | select(.devid=="0x1F15")' < /usr/share/doc/nvidia-driver-460/supported-gpus.json
{
  "devid": "0x1F15",
  "name": "GeForce RTX 2060",
  "features": [
    "dpycbcr420",
    "dpgsynccompatible",
    "hdmi4k60rgb444",
    "hdmigsynccompatible",
    "geforce",
    "runtimepm",
    "vdpaufeaturesetJ"
  ]
}
```

[GLVND]: https://gitlab.freedesktop.org/glvnd/libglvnd

### Compute

The integrated graphics controller is used exclusively for rendering. The dGPU
is made available as a compute node.

## Power Profiles

### Balanced

- Set the sync data to disk to 15s
- Enables laptop mode feature in kernel
- Enables SCSI/SATA link time power management
- Controls the Intel PState values if they exist

### Performance

- Uses settings from Balanced
- Uses ACPI Platform profile if the hardware is supported by the kernel

### Battery

- Uses settings from Performance
- Sets Screen brightness to a lower value
- Turns keyboard backlight off

## Hotplug detection

The dbus signal `HotPlugDetect` is sent when a display is plugged into a port
connected to the dGPU. If in integrated or compute mode, the
[GNOME extension] will prompt to switch to hybrid mode so the display
can be used.

[GNOME extension]: https://github.com/pop-os/gnome-shell-extension-system76-power

### Adding hotplug detection

#### Intel-based systems

The GPIO (sideband) port and pins for the display ports can be determined with
the schematics and output of [coreboot-collector]. The schematics will indicate
which GPIOs are display ports (`*_HPD`). The corresponding `GPP_*` entry in
`coreboot-collector.txt` will have the port/pin tuple.

##### Muxed DisplayPort

Some models have muxed DisplayPort ouput from mDP and USB-C. These units have a
separate data switch pin that is used to determine which output is used.

#### AMD-based systems

A MMIO region for FCH GPIO controls is used to detect external display plug
events. Display ports use `*_HPD` as Intel systems, but may not map to a
literal GPIO (e.g., `HDMI_HPD` maps to `DP3_HPD` on kudu6). Generating a diff
from coreboot-collector in NVIDIA mode before and after plugging in a display
should provide the GPIO number.

[coreboot-collector]: https://github.com/system76/coreboot-collector
