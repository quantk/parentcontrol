# Minecraft detection fixtures

Each `.args` file is one process command line, one argument per line.
Lines starting with `#` are comments. Every file in `game/` must be detected
as the Minecraft client, every file in `not-game/` must not; the test in
`crates/core/src/rules/minecraft/tests.rs` picks up new files automatically.

Files marked "Synthesized" were written from the launchers' argument
templates. Replace them with real captures (PLAN.md, section 9.2):

```sh
# Linux, while the game runs (the PID is the java process, not the launcher):
tr '\0' '\n' < /proc/<pid>/cmdline > game/<launcher>-<os>-<loader>.args
```

```powershell
# Windows: copy CommandLine, then put one argument per line.
Get-CimInstance Win32_Process -Filter "ProcessId=<pid>" | Select-Object -Expand CommandLine
```

Redact `--accessToken`, `--uuid`, `--xuid` and the user name before committing.
