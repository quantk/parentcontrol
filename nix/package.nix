# The agent and the tray, with the systemd unit and the tray autostart
# entry from deploy/linux pointing at this package. The server is shipped
# as a container image (deploy/Dockerfile) instead.
{
  lib,
  rustPlatform,
}:

rustPlatform.buildRustPackage {
  pname = "parentcontrol";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
      ../deploy/linux
      ../docs
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;

  cargoBuildFlags = [
    "-p"
    "parentcontrol-agent"
    "-p"
    "parentcontrol-tray"
  ];
  cargoTestFlags = [
    "-p"
    "parentcontrol-core"
    "-p"
    "parentcontrol-agent"
    "-p"
    "parentcontrol-tray"
  ];

  postInstall = ''
    install -Dm644 deploy/linux/parentcontrol-agent.service \
      $out/lib/systemd/system/parentcontrol-agent.service
    substituteInPlace $out/lib/systemd/system/parentcontrol-agent.service \
      --replace-fail /usr/local/bin/parentcontrol-agent $out/bin/parentcontrol-agent

    install -Dm644 deploy/linux/parentcontrol-tray.desktop \
      $out/etc/xdg/autostart/parentcontrol-tray.desktop
    substituteInPlace $out/etc/xdg/autostart/parentcontrol-tray.desktop \
      --replace-fail "Exec=parentcontrol-tray" "Exec=$out/bin/parentcontrol-tray"
  '';

  meta = {
    description = "Shared time limits for selected programs on home PCs";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "parentcontrol-agent";
  };
}
