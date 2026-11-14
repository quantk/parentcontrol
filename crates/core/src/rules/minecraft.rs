//! Recognising the Minecraft Java Edition client.
//!
//! The game is a JVM whose executable may be renamed or bundled, so the
//! executable name is ignored. Launchers (the official one, Prism's window)
//! are not the game and must not match. Signals are tried from cheap to
//! expensive: command line first, then files the process has loaded.

use crate::process::{ProcessInfo, ProcessInspector};

/// Main classes of the client itself, of mod loaders wrapping it and of
/// launcher entry points that load it into the same JVM.
const MAIN_CLASSES: &[&str] = &[
    "net.minecraft.client.main.Main",
    "net.minecraft.launchwrapper.Launch",
    "net.fabricmc.loader.impl.launch.knot.KnotClient",
    "net.fabricmc.loader.launch.knot.KnotClient",
    "org.quiltmc.loader.impl.launch.knot.KnotClient",
    "cpw.mods.modlauncher.Launcher",
    "cpw.mods.bootstraplauncher.BootstrapLauncher",
    "net.minecraftforge.bootstrap.ForgeBootstrap",
    "net.neoforged.fml.startup.Client",
    "org.prismlauncher.EntryPoint",
    "org.multimc.EntryPoint",
];

/// Arguments the client receives from every launcher.
const GAME_ARGUMENTS: &[&str] = &["--assetsDir", "--gameDir", "--accessToken"];

/// JVM system properties set only for the client.
const GAME_PROPERTIES: &[&str] = &["-Dminecraft.launcher.brand=", "-Dminecraft.client.jar="];

/// Options whose value is a classpath.
const CLASSPATH_OPTIONS: &[&str] = &["-cp", "-classpath", "--class-path"];

pub(super) fn is_minecraft(process: &ProcessInfo, inspector: &dyn ProcessInspector) -> bool {
    has_game_command_line(&process.command_line)
        || inspector
            .loaded_files(process.id)
            .iter()
            .any(|file| is_client_jar(&file.to_string_lossy()))
}

fn has_game_command_line(arguments: &[String]) -> bool {
    // Skip argv[0]: a renamed executable is exactly what we do not trust.
    let arguments = arguments.get(1..).unwrap_or_default();
    arguments.iter().any(|argument| {
        MAIN_CLASSES.contains(&argument.as_str())
            || GAME_ARGUMENTS.contains(&argument.as_str())
            || GAME_PROPERTIES
                .iter()
                .any(|property| argument.starts_with(property))
    }) || classpaths(arguments).any(|classpath| split_classpath(classpath).any(is_client_jar))
}

/// Values of every classpath option on the command line.
fn classpaths(arguments: &[String]) -> impl Iterator<Item = &str> {
    let separate = arguments
        .windows(2)
        .filter(|pair| CLASSPATH_OPTIONS.contains(&pair[0].as_str()))
        .map(|pair| pair[1].as_str());
    let inline = arguments.iter().filter_map(|argument| {
        argument
            .strip_prefix("-Djava.class.path=")
            .or_else(|| argument.strip_prefix("--class-path="))
    });
    separate.chain(inline)
}

/// Splits a classpath with the separator of the OS that produced it:
/// `;` on Windows, where `:` also appears after drive letters, `:` elsewhere.
fn split_classpath(classpath: &str) -> impl Iterator<Item = &str> {
    let separator = if classpath.contains(';') { ';' } else { ':' };
    classpath.split(separator).filter(|entry| !entry.is_empty())
}

/// Whether `path` is a Minecraft client jar in any launcher's layout:
///
/// * official launcher and `MultiMC`: `versions/<id>/<id>.jar`;
/// * Prism: `libraries/com/mojang/minecraft/<v>/minecraft-<v>-client.jar`;
/// * Forge and `NeoForge`: `libraries/net/minecraft/client/<v>/client-<v>-*.jar`.
fn is_client_jar(path: &str) -> bool {
    let components: Vec<&str> = path.split(['/', '\\']).filter(|c| !c.is_empty()).collect();
    let Some((file, parents)) = components.split_last() else {
        return false;
    };
    let Some(stem) = jar_stem(file) else {
        return false;
    };

    let in_versions_dir = matches!(parents, [.., versions, id] if versions.eq_ignore_ascii_case("versions") && id == &stem);
    let prism_client = stem.starts_with("minecraft-") && stem.ends_with("-client");
    let forge_client = parents
        .windows(3)
        .any(|w| w == ["net", "minecraft", "client"]);
    in_versions_dir || prism_client || forge_client
}

/// The file name without a case-insensitive `.jar` extension.
fn jar_stem(file: &str) -> Option<&str> {
    let split = file.len().checked_sub(".jar".len())?;
    let (stem, extension) = (file.get(..split)?, file.get(split..)?);
    extension.eq_ignore_ascii_case(".jar").then_some(stem)
}

#[cfg(test)]
mod tests;
