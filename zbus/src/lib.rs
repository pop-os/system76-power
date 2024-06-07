// SPDX-License-Identifier: MPL-2.0

use serde::{Deserialize, Serialize};
use zvariant::Type;

#[derive(Deserialize, Serialize, Type, Debug)]
pub struct ChargeProfile {
    pub id:          String,
    pub title:       String,
    pub description: String,
    pub start:       u8,
    pub end:         u8,
}

#[zbus::dbus_proxy(
    interface = "com.system76.PowerDaemon",
    default_service = "com.system76.PowerDaemon",
    default_path = "/com/system76/PowerDaemon"
)]
trait PowerDaemon {
    /// Balanced method
    fn balanced(&self) -> zbus::Result<()>;

    /// Battery method
    fn battery(&self) -> zbus::Result<()>;

    /// Performance method
    fn performance(&self) -> zbus::Result<()>;

    /// GetProfile method
    fn get_profile(&self) -> zbus::Result<String>;

    /// GetExternalDisplaysRequireDGPU method
    fn get_external_displays_require_dgpu(&self) -> zbus::Result<bool>;

    /// GetDefaultGraphics method
    fn get_default_graphics(&self) -> zbus::Result<String>;

    /// GetGraphics method
    fn get_graphics(&self) -> zbus::Result<String>;

    /// SetGraphics method
    fn set_graphics(&self, vendor: &str) -> zbus::Result<()>;

    /// GetSwitchable method
    fn get_switchable(&self) -> zbus::Result<bool>;

    /// GetDesktop method
    fn get_desktop(&self) -> zbus::Result<bool>;

    /// GetGraphicsPower method
    fn get_graphics_power(&self) -> zbus::Result<bool>;

    /// SetGraphicsPower method
    fn set_graphics_power(&self, power: bool) -> zbus::Result<()>;

    /// AutoGraphicsPower
    fn auto_graphics_power(&self) -> zbus::Result<()>;

    /// GetChargeProfiles method
    fn get_charge_profiles(&self) -> zbus::Result<Vec<ChargeProfile>>;

    /// GetChargeThresholds method
    fn get_charge_thresholds(&self) -> zbus::Result<(u8, u8)>;

    /// SetChargeThresholds method
    fn set_charge_thresholds(&self, thresholds: &(u8, u8)) -> zbus::Result<()>;

    /// HotPlugDetect signal
    #[dbus_proxy(signal)]
    fn hot_plug_detect(&self, port: u64) -> zbus::Result<()>;

    /// PowerProfileSwitch signal
    #[dbus_proxy(signal)]
    fn power_profile_switch(&self, profile: &str) -> zbus::Result<()>;
}
