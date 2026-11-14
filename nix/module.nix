# NixOS module: the agent as a system service and the tray in every
# graphical session. Enroll once after enabling:
#
#   sudo parentcontrol-agent enroll --server https://192.168.1.10:8443 --code XXXX-XXXX
{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.parentcontrol;
in
{
  options.services.parentcontrol = {
    enable = lib.mkEnableOption "the parentcontrol agent";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "parentcontrol.packages.\${system}.default";
      description = "The package with the agent and the tray.";
    };

    tray = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Whether to start the tray in every graphical session.";
    };
  };

  config = lib.mkIf cfg.enable {
    # `enroll` is run by hand once.
    environment.systemPackages = [ cfg.package ];

    # The unit, with its sandboxing, comes from the package so it is the
    # same as on other distributions.
    systemd.packages = [ cfg.package ];
    systemd.services.parentcontrol-agent.wantedBy = [ "multi-user.target" ];

    environment.etc."xdg/autostart/parentcontrol-tray.desktop" = lib.mkIf cfg.tray {
      source = "${cfg.package}/etc/xdg/autostart/parentcontrol-tray.desktop";
    };
  };
}
