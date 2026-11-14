#!/bin/sh
# Installs the agent and the tray on a Linux PC without Nix.
# Run as root from a directory holding parentcontrol-agent,
# parentcontrol-tray and the files of deploy/linux:
#
#   sudo ./install.sh
#
# Then enroll the PC with the code from the panel:
#
#   sudo parentcontrol-agent enroll --server https://192.168.1.135:8443 --code XXXX-XXXX
set -eu

here=$(dirname "$0")

if [ "$(id -u)" -ne 0 ]; then
    echo "Run as root: sudo $0" >&2
    exit 1
fi

install -m 0755 "$here/parentcontrol-agent" /usr/local/bin/parentcontrol-agent
install -m 0755 "$here/parentcontrol-tray" /usr/local/bin/parentcontrol-tray
install -m 0644 "$here/parentcontrol-agent.service" /etc/systemd/system/parentcontrol-agent.service
install -d -m 0755 /etc/xdg/autostart
install -m 0644 "$here/parentcontrol-tray.desktop" /etc/xdg/autostart/parentcontrol-tray.desktop

systemctl daemon-reload
systemctl enable --now parentcontrol-agent.service
# Pick up a new binary when upgrading.
systemctl restart parentcontrol-agent.service

echo "Agent installed and running. The tray starts at the next login."
