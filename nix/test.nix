# A VM with the NixOS module: the agent, sandboxed by its systemd unit,
# must see and end a program of a child's account and keep counting
# across a restart, without touching the parent's programs.
{ self, pkgs }:

let
  marker = "parentcontrol-vm-test-game";
  week =
    day:
    pkgs.lib.genAttrs [
      "monday"
      "tuesday"
      "wednesday"
      "thursday"
      "friday"
      "saturday"
      "sunday"
    ] (_: day);
  policy = pkgs.writeText "policy.json" (
    builtins.toJSON {
      time_zone = "UTC";
      day_start = "04:00";
      accounts = [ "kid" ];
      groups = [
        {
          id = 1;
          name = "Игры";
          rules = [ { command_line_contains = marker; } ];
          schedule = week { quota = 1; };
        }
      ];
    }
  );
  game = "${pkgs.python3}/bin/python3 -c 'import time; time.sleep(3600)' ${marker}";
in
pkgs.testers.nixosTest {
  name = "parentcontrol-agent";

  nodes.machine = {
    imports = [ self.nixosModules.default ];
    services.parentcontrol.enable = true;
    users.users.kid.isNormalUser = true;
    users.users.parent.isNormalUser = true;
  };

  testScript = ''
    machine.wait_for_unit("multi-user.target")
    machine.succeed("install -m 600 ${policy} /var/lib/parentcontrol/policy.json")

    with subtest("the parent's program is left alone"):
        machine.succeed("su parent -c \"${game} >/dev/null 2>&1 &\"")

    with subtest("the child's program is closed when the quota runs out"):
        machine.succeed("su kid -c \"${game} >/dev/null 2>&1 &\"")
        machine.wait_until_fails("pgrep -u kid -f ${marker}", timeout=120)
        machine.succeed("journalctl -u parentcontrol-agent | grep -q 'mode=Graceful'")

    with subtest("usage survives a restart; a relaunch dies at once"):
        machine.succeed("systemctl restart parentcontrol-agent")
        machine.succeed("su kid -c \"${game} >/dev/null 2>&1 &\"")
        machine.wait_until_fails("pgrep -u kid -f ${marker}", timeout=15)
        machine.succeed("journalctl -u parentcontrol-agent | grep -q 'launch denied'")

    with subtest("the parent's program still runs"):
        machine.succeed("pgrep -u parent -f ${marker}")

    with subtest("the state directory is private"):
        machine.succeed("test \"$(stat -c %a /var/lib/parentcontrol)\" = 700")
        machine.fail("su kid -c 'cat /var/lib/parentcontrol/journal.json'")
  '';
}
