# System76 Power Management

**system76-power** is a utility for managing graphics and power profiles.

## Graphics Modes

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
You can also launch apps with the wrapper script `prime-run`, which sets these
variables for you.

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
