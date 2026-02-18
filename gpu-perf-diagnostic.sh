#!/bin/bash
# GPU Performance Diagnostic Script
# Analyzes system76-power vs TLP conflicts affecting integrated GPU performance
# Author: System76-power analysis tool
# Usage: ./gpu-perf-diagnostic.sh [output-file]

set -euo pipefail

# Output file
OUTPUT_FILE="${1:-gpu-perf-diagnostic-$(date +%Y%m%d-%H%M%S).txt}"

# Colors for terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper function to print and log
log() {
    echo -e "$1" | tee -a "$OUTPUT_FILE"
}

log_header() {
    echo "" | tee -a "$OUTPUT_FILE"
    echo "==================================================" | tee -a "$OUTPUT_FILE"
    echo "$1" | tee -a "$OUTPUT_FILE"
    echo "==================================================" | tee -a "$OUTPUT_FILE"
}

log_section() {
    echo "" | tee -a "$OUTPUT_FILE"
    echo "### $1 ###" | tee -a "$OUTPUT_FILE"
}

# Function to safely read sysfs files
read_sysfs() {
    local file="$1"
    if [ -f "$file" ]; then
        cat "$file" 2>/dev/null || echo "ERROR: Cannot read $file"
    else
        echo "NOT_FOUND"
    fi
}

# Check if running as root (needed for some files)
if [ "$EUID" -ne 0 ]; then
    log "${YELLOW}Warning: Not running as root. Some information may be unavailable.${NC}"
    log "${YELLOW}For complete output, run: sudo ./gpu-perf-diagnostic.sh${NC}"
fi

log_header "GPU PERFORMANCE DIAGNOSTIC - $(date)"
log "Hostname: $(hostname)"
log "Kernel: $(uname -r)"
log ""

# =============================================================================
# 1. POWER SOURCE
# =============================================================================
log_section "POWER SOURCE"

power_source="UNKNOWN"
for ac in /sys/class/power_supply/AC* /sys/class/power_supply/ACAD; do
    if [ -e "$ac/online" ]; then
        online=$(read_sysfs "$ac/online")
        if [ "$online" = "1" ]; then
            power_source="AC"
            log "${GREEN}⚡ Power Source: AC CONNECTED${NC}"
        else
            power_source="BATTERY"
            log "${YELLOW}🔋 Power Source: BATTERY${NC}"
        fi
        break
    fi
done

if [ "$power_source" = "UNKNOWN" ]; then
    log "${RED}⚠️  Could not detect power source${NC}"
fi

# =============================================================================
# 2. SYSTEM76-POWER STATUS
# =============================================================================
log_section "SYSTEM76-POWER PROFILE"

if command -v system76-power &> /dev/null; then
    current_profile=$(system76-power profile 2>/dev/null || echo "ERROR")
    log "Current Profile: ${BLUE}$current_profile${NC}"
    
    # Check if daemon is running
    if systemctl is-active --quiet system76-power; then
        log "Daemon Status: ${GREEN}RUNNING${NC}"
    else
        log "Daemon Status: ${RED}NOT RUNNING${NC}"
    fi
else
    log "${RED}system76-power: NOT INSTALLED${NC}"
fi

# =============================================================================
# 3. CPU PARAMETERS
# =============================================================================
log_section "CPU PARAMETERS (AMD Ryzen 9 5900HX)"

# CPU Driver
cpu_driver=$(read_sysfs /sys/devices/system/cpu/cpu0/cpufreq/scaling_driver)
log "Frequency Driver: $cpu_driver"

# Governor
governor=$(read_sysfs /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)
log "Governor: $governor"

# Frequencies
current_freq=$(read_sysfs /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq)
min_freq=$(read_sysfs /sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq)
max_freq=$(read_sysfs /sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq)
hw_max_freq=$(read_sysfs /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq)

log "Current Frequency: $((current_freq / 1000)) MHz"
log "Min Frequency: $((min_freq / 1000)) MHz"
log "Max Frequency (scaling): $((max_freq / 1000)) MHz"
log "Max Frequency (hardware): $((hw_max_freq / 1000)) MHz"

# Calculate frequency cap percentage
if [ "$hw_max_freq" != "NOT_FOUND" ] && [ "$max_freq" != "NOT_FOUND" ]; then
    if [ "$hw_max_freq" -gt 0 ]; then
        percent=$((max_freq * 100 / hw_max_freq))
        if [ "$percent" -lt 95 ]; then
            log "${RED}⚠️  CPU FREQUENCY CAPPED AT ${percent}% of maximum!${NC}"
            log "${RED}   This severely limits integrated GPU performance!${NC}"
        else
            log "${GREEN}✓ CPU frequency not capped (${percent}%)${NC}"
        fi
    fi
fi

# CPU Boost
boost=$(read_sysfs /sys/devices/system/cpu/cpufreq/boost)
if [ "$boost" = "1" ]; then
    log "${GREEN}✓ CPU Boost: ENABLED${NC}"
elif [ "$boost" = "0" ]; then
    log "${RED}⚠️  CPU Boost: DISABLED (limits performance)${NC}"
else
    log "CPU Boost: $boost"
fi

# Energy Performance Preference (EPP) for AMD
if [ -f /sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference ]; then
    epp=$(read_sysfs /sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference)
    log "Energy Performance Preference: $epp"
fi

# Show all CPU frequencies for verification
log ""
log "Per-CPU Frequencies (first 8 cores):"
for cpu in {0..7}; do
    if [ -f /sys/devices/system/cpu/cpu$cpu/cpufreq/scaling_cur_freq ]; then
        freq=$(read_sysfs /sys/devices/system/cpu/cpu$cpu/cpufreq/scaling_cur_freq)
        log "  CPU$cpu: $((freq / 1000)) MHz"
    fi
done

# =============================================================================
# 4. AMD RADEON VEGA GPU
# =============================================================================
log_section "AMD RADEON VEGA INTEGRATED GPU"

amd_card_found=false
for card in /sys/class/drm/card[0-9]; do
    if [ -e "$card/device/vendor" ]; then
        vendor=$(read_sysfs "$card/device/vendor")
        if [ "$vendor" = "0x1002" ]; then  # AMD vendor ID
            amd_card_found=true
            cardname=$(basename "$card")
            log "Found AMD GPU: ${GREEN}$cardname${NC}"
            
            # Device ID
            device_id=$(read_sysfs "$card/device/device")
            log "Device ID: $device_id"
            
            # Power DPM Force Performance Level
            dpm_perf=$(read_sysfs "$card/device/power_dpm_force_performance_level")
            if [ "$dpm_perf" = "auto" ]; then
                log "${GREEN}✓ DPM Force Performance: $dpm_perf${NC}"
            elif [ "$dpm_perf" = "low" ]; then
                log "${RED}⚠️  DPM Force Performance: $dpm_perf (limits GPU clocks!)${NC}"
            else
                log "DPM Force Performance: $dpm_perf"
            fi
            
            # Power DPM State
            dpm_state=$(read_sysfs "$card/device/power_dpm_state")
            log "DPM State: $dpm_state"
            
            # Power Method
            power_method=$(read_sysfs "$card/device/power_method")
            log "Power Method: $power_method"
            
            # Power Profile
            power_profile=$(read_sysfs "$card/device/power_profile")
            log "Power Profile: $power_profile"
            
            # GPU Clocks
            log ""
            log "--- GPU Clock States (active marked with *) ---"
            
            if [ -f "$card/device/pp_dpm_sclk" ]; then
                log "Shader/Core Clock (SCLK):"
                read_sysfs "$card/device/pp_dpm_sclk" | while IFS= read -r line; do
                    if [[ "$line" == *"*"* ]]; then
                        log "  ${GREEN}$line${NC}"
                    else
                        log "  $line"
                    fi
                done
            fi
            
            if [ -f "$card/device/pp_dpm_mclk" ]; then
                log "Memory Clock (MCLK):"
                read_sysfs "$card/device/pp_dpm_mclk" | while IFS= read -r line; do
                    if [[ "$line" == *"*"* ]]; then
                        log "  ${GREEN}$line${NC}"
                    else
                        log "  $line"
                    fi
                done
            fi
            
            if [ -f "$card/device/pp_dpm_socclk" ]; then
                log "SOC Clock (affects memory controller):"
                read_sysfs "$card/device/pp_dpm_socclk" | while IFS= read -r line; do
                    if [[ "$line" == *"*"* ]]; then
                        log "  ${GREEN}$line${NC}"
                    else
                        log "  $line"
                    fi
                done
            fi
            
            if [ -f "$card/device/pp_dpm_fclk" ]; then
                log "Data Fabric Clock (FCLK - Infinity Fabric):"
                read_sysfs "$card/device/pp_dpm_fclk" | while IFS= read -r line; do
                    if [[ "$line" == *"*"* ]]; then
                        log "  ${GREEN}$line${NC}"
                    else
                        log "  $line"
                    fi
                done
            fi
            
            # Runtime PM
            if [ -f "$card/device/power/control" ]; then
                pm_control=$(read_sysfs "$card/device/power/control")
                log ""
                log "Runtime PM: $pm_control"
            fi
        fi
    fi
done

if [ "$amd_card_found" = false ]; then
    log "${RED}⚠️  No AMD GPU found!${NC}"
fi

# =============================================================================
# 5. POWER MANAGEMENT SETTINGS
# =============================================================================
log_section "SYSTEM POWER MANAGEMENT"

# PCIe ASPM
aspm=$(read_sysfs /sys/module/pcie_aspm/parameters/policy)
if [[ "$aspm" == *"[default]"* ]]; then
    log "${GREEN}✓ PCIe ASPM: default${NC}"
elif [[ "$aspm" == *"[powersupersave]"* ]]; then
    log "${YELLOW}⚠️  PCIe ASPM: powersupersave (may limit bandwidth)${NC}"
else
    log "PCIe ASPM: $aspm"
fi

# Laptop Mode
laptop_mode=$(read_sysfs /proc/sys/vm/laptop_mode)
log "Laptop Mode: $laptop_mode"

# Dirty Writeback
dirty_writeback=$(read_sysfs /proc/sys/vm/dirty_writeback_centisecs)
log "Dirty Writeback: $((dirty_writeback / 100)) seconds"

# NMI Watchdog
nmi_watchdog=$(read_sysfs /proc/sys/kernel/nmi_watchdog)
log "NMI Watchdog: $nmi_watchdog"

# =============================================================================
# 6. STORAGE POWER MANAGEMENT
# =============================================================================
log_section "STORAGE POWER MANAGEMENT"

log "SATA/SCSI Link Power Management:"
for host in /sys/class/scsi_host/host*; do
    if [ -f "$host/link_power_management_policy" ]; then
        hostnum=$(basename "$host")
        policy=$(read_sysfs "$host/link_power_management_policy")
        log "  $hostnum: $policy"
    fi
done

# NVMe APST
log ""
log "NVMe APST Status:"
if command -v nvme &> /dev/null; then
    for nvme_dev in /dev/nvme[0-9]; do
        if [ -e "$nvme_dev" ]; then
            log "  Device: $nvme_dev"
            nvme get-feature -f 0x0c "$nvme_dev" -H 2>/dev/null | grep -i "autonomous" || log "    APST feature query failed"
        fi
    done
else
    log "  nvme-cli not installed (cannot check APST)"
fi

# =============================================================================
# 7. MEMORY INFORMATION
# =============================================================================
log_section "MEMORY & SWAP"

log "Memory Info (affects integrated GPU):"
free -h | tee -a "$OUTPUT_FILE"

log ""
log "Active Swap Devices:"
swapon --show 2>/dev/null | tee -a "$OUTPUT_FILE" || log "  No swap active"

# Check for zswap/zram
if [ -d /sys/module/zswap ]; then
    zswap_enabled=$(read_sysfs /sys/module/zswap/parameters/enabled)
    log ""
    log "zswap: $zswap_enabled"
    if [ "$zswap_enabled" = "Y" ]; then
        log "  Compressor: $(read_sysfs /sys/module/zswap/parameters/compressor)"
        log "  Max pool: $(read_sysfs /sys/module/zswap/parameters/max_pool_percent)%"
    fi
fi

# =============================================================================
# 8. TLP STATUS
# =============================================================================
log_section "TLP STATUS & CONFIGURATION"

if command -v tlp-stat &> /dev/null; then
    log "${YELLOW}TLP is installed${NC}"
    
    # Check if TLP service is enabled
    if systemctl is-enabled --quiet tlp 2>/dev/null; then
        log "TLP Service: ${GREEN}ENABLED${NC}"
    else
        log "TLP Service: ${YELLOW}DISABLED${NC}"
    fi
    
    # Check if TLP is active
    if systemctl is-active --quiet tlp 2>/dev/null; then
        log "TLP Service: ${GREEN}ACTIVE${NC}"
    else
        log "TLP Service: ${RED}INACTIVE${NC}"
    fi
    
    log ""
    log "TLP Status Summary:"
    tlp-stat -s 2>/dev/null | head -30 | tee -a "$OUTPUT_FILE" || log "  Error running tlp-stat"
    
    log ""
    log "TLP Configuration (relevant settings):"
    if [ -f /etc/tlp.conf ]; then
        grep -E "^(CPU_|RADEON_|PCIE_|SATA_|WIFI_|SOUND_)" /etc/tlp.conf 2>/dev/null | grep -v "^#" | tee -a "$OUTPUT_FILE" || log "  No active settings found"
    else
        log "  /etc/tlp.conf not found"
    fi
else
    log "${GREEN}TLP not installed${NC}"
fi

# =============================================================================
# 9. RECENT TLP ACTIVITY
# =============================================================================
log_section "RECENT TLP ACTIVITY"

if systemctl list-units --type=service | grep -q tlp; then
    log "Last 30 TLP journal entries:"
    journalctl -u tlp.service -n 30 --no-pager 2>/dev/null | tee -a "$OUTPUT_FILE" || log "  No journal entries found"
else
    log "TLP service not found in systemd"
fi

# =============================================================================
# 10. ANALYSIS & RECOMMENDATIONS
# =============================================================================
log_section "ANALYSIS & RECOMMENDATIONS"

issues_found=false

# Check CPU frequency cap
if [ "$hw_max_freq" != "NOT_FOUND" ] && [ "$max_freq" != "NOT_FOUND" ]; then
    if [ "$hw_max_freq" -gt 0 ]; then
        percent=$((max_freq * 100 / hw_max_freq))
        if [ "$percent" -lt 95 ]; then
            issues_found=true
            log "${RED}❌ ISSUE: CPU frequency capped at ${percent}%${NC}"
            log "   Impact: Severely limits integrated GPU performance"
            log "   Cause: Likely system76-power Battery profile (60% cap)"
            log "   Solution: Switch to Performance profile or disable TLP's CPU management"
        fi
    fi
fi

# Check CPU boost
if [ "$boost" = "0" ]; then
    issues_found=true
    log "${RED}❌ ISSUE: CPU Boost is DISABLED${NC}"
    log "   Impact: Reduces package power budget for integrated GPU"
    log "   Cause: Battery profile or TLP setting CPU_BOOST_ON_BAT=0"
    log "   Solution: Enable boost or use Performance profile"
fi

# Check GPU DPM
if [ "$dpm_perf" = "low" ]; then
    issues_found=true
    log "${RED}❌ ISSUE: GPU DPM set to 'low'${NC}"
    log "   Impact: Limits GPU clock frequencies"
    log "   Cause: TLP's RADEON_DPM_PERF_LEVEL_ON_BAT=low or Battery profile"
    log "   Solution: Set to 'auto' or disable TLP's Radeon management"
fi

# Check PCIe ASPM
if [[ "$aspm" == *"[powersupersave]"* ]]; then
    log "${YELLOW}⚠️  WARNING: PCIe ASPM in powersupersave mode${NC}"
    log "   Impact: May reduce PCIe bandwidth"
    log "   Cause: system76-power Battery profile or TLP"
    log "   Solution: Set to 'default' for better performance"
fi

# Check power source vs profile mismatch
if [ "$power_source" = "BATTERY" ] && [ "$current_profile" = "performance" ]; then
    log "${YELLOW}⚠️  NOTE: Performance profile active on battery${NC}"
    log "   Expected behavior: High performance but shorter battery life"
    log "   If FPS is still low, TLP may be overriding settings"
fi

# Check for TLP conflicts
if command -v tlp-stat &> /dev/null && systemctl is-active --quiet tlp 2>/dev/null; then
    if command -v system76-power &> /dev/null; then
        issues_found=true
        log "${RED}❌ CONFLICT: Both TLP and system76-power are active${NC}"
        log "   Impact: Settings conflict, unpredictable behavior"
        log "   Cause: Both tools manage CPU/GPU power settings"
        log "   Solution: Disable TLP's conflicting settings in /etc/tlp.conf"
        log "   Recommended: Comment out CPU_*, RADEON_*, PCIE_* settings"
    fi
fi

if [ "$issues_found" = false ]; then
    log "${GREEN}✓ No obvious performance issues detected${NC}"
fi

# =============================================================================
# 11. QUICK REFERENCE
# =============================================================================
log_section "QUICK REFERENCE - Expected Values"

log "For HIGH PERFORMANCE on integrated GPU:"
log "  • Power Source: Any (AC or Battery)"
log "  • system76-power Profile: Performance"
log "  • CPU Frequency Cap: 100% ($(( hw_max_freq / 1000 )) MHz)"
log "  • CPU Boost: ENABLED (1)"
log "  • Radeon DPM: auto or high"
log "  • PCIe ASPM: default"
log ""
log "For BATTERY LIFE optimization:"
log "  • system76-power Profile: Battery"
log "  • CPU Frequency Cap: 60% (limits to ~2700 MHz)"
log "  • CPU Boost: DISABLED (0)"
log "  • Radeon DPM: low"
log "  • PCIe ASPM: powersupersave"

# =============================================================================
# FOOTER
# =============================================================================
log_header "DIAGNOSTIC COMPLETE"

log "Output saved to: ${GREEN}$OUTPUT_FILE${NC}"
log ""
log "Next Steps:"
log "1. Review the ANALYSIS & RECOMMENDATIONS section above"
log "2. Test FPS in your game and note the value"
log "3. Try different scenarios:"
log "   - system76-power profile battery (note FPS)"
log "   - system76-power profile performance (note FPS)"
log "   - Plug/unplug AC (note FPS change)"
log "4. Compare the parameter changes in each scenario"
log ""
log "To test TLP conflict:"
log "   sudo systemctl stop tlp"
log "   system76-power profile performance"
log "   # Test FPS - should be high even on battery now"
log ""

echo ""
echo "Summary written to: $OUTPUT_FILE"
echo "You can share this file for further analysis."
