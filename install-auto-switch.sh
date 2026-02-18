#!/bin/bash
# AC Auto-Switching Installation Script
# Installs the patched system76-power with AC auto-switching feature

set -e

echo "========================================="
echo "System76-Power AC Auto-Switching Install"
echo "========================================="
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then 
    echo "ERROR: Please run as root (sudo ./install-auto-switch.sh)"
    exit 1
fi

# Check if binary exists
if [ ! -f "target/release/system76-power" ]; then
    echo "ERROR: Binary not found. Please run 'cargo build --release' first."
    exit 1
fi

echo "This script will:"
echo "  1. Stop the system76-power daemon"
echo "  2. Backup the current binary"
echo "  3. Install the new binary with AC auto-switching"
echo "  4. Install the configuration file"
echo "  5. Restart the daemon"
echo ""
read -p "Continue? [y/N] " -n 1 -r
echo ""

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Installation cancelled."
    exit 0
fi

echo ""
echo "[1/5] Stopping system76-power daemon..."
systemctl stop system76-power

echo "[2/5] Backing up original binary..."
if [ ! -f "/usr/bin/system76-power.backup" ]; then
    cp /usr/bin/system76-power /usr/bin/system76-power.backup
    echo "      Backup created: /usr/bin/system76-power.backup"
else
    echo "      Backup already exists, skipping"
fi

echo "[3/5] Installing new binary..."
cp target/release/system76-power /usr/bin/system76-power
chmod +x /usr/bin/system76-power
echo "      Installed: /usr/bin/system76-power"

echo "[4/5] Installing configuration file..."
if [ ! -f "/etc/system76-power.conf" ]; then
    cp system76-power.conf /etc/system76-power.conf
    echo "      Installed: /etc/system76-power.conf"
else
    echo "      Configuration file already exists"
    read -p "      Overwrite? [y/N] " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        cp system76-power.conf /etc/system76-power.conf
        echo "      Configuration file updated"
    else
        echo "      Keeping existing configuration"
    fi
fi

echo "[5/5] Restarting daemon..."
systemctl start system76-power
sleep 2

echo ""
echo "========================================="
echo "Installation Complete!"
echo "========================================="
echo ""

# Check daemon status
if systemctl is-active --quiet system76-power; then
    echo "✅ Daemon is running"
    
    # Show relevant logs
    echo ""
    echo "Recent logs:"
    echo "----------------------------------------"
    journalctl -u system76-power -n 20 --no-pager | grep -i "auto\|power source\|profile" || true
    echo "----------------------------------------"
    echo ""
    
    # Show current status
    echo "Current status:"
    echo "  Profile: $(system76-power profile 2>/dev/null || echo 'unknown')"
    
    if [ -f "/sys/class/power_supply/AC0/online" ]; then
        AC_STATUS=$(cat /sys/class/power_supply/AC0/online)
        if [ "$AC_STATUS" = "1" ]; then
            echo "  AC Power: Connected"
        else
            echo "  AC Power: Disconnected (on battery)"
        fi
    else
        echo "  AC Power: Could not detect"
    fi
    
    echo ""
    echo "Auto-switching feature installed successfully!"
    echo ""
    echo "Next steps:"
    echo "  1. Read AC_AUTO_SWITCH_TESTING.md for testing instructions"
    echo "  2. Test by plugging/unplugging AC adapter"
    echo "  3. Monitor logs: sudo journalctl -u system76-power -f"
    echo ""
    echo "To disable auto-switching:"
    echo "  Edit /etc/system76-power.conf and set 'enabled = false'"
    echo ""
else
    echo "❌ ERROR: Daemon failed to start"
    echo ""
    echo "Check logs with:"
    echo "  sudo journalctl -u system76-power -n 50"
    echo ""
    exit 1
fi
