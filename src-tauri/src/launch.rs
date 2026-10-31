//! Construction de la ligne de commande Java (Minecraft + NeoForge) et options.txt.
use crate::auth::Session;
use crate::install::{neoforge_id, os_name, rules_allow};
use crate::paths;
use crate::settings::Settings;
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

const OPTIMIZED_JVM: &[&str] = &[
    "-XX:+UseG1GC",
    "-XX:+ParallelRefProcEnabled",
    "-XX:MaxGCPauseMillis=200",
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+DisableExplicitGC",
    "-XX:G1NewSizePercent=30",
    "-XX:G1MaxNewSizePercent=40",
    "-XX:G1HeapRegionSize=8M",
    "-XX:G1ReservePercent=20",
    "-XX:G1MixedGCCountTarget=4",
    "-XX:InitiatingHeapOccupancyPercent=15",
    "-XX:SurvivorRatio=32",
    "-XX:+PerfDisableSharedMem",
    "-XX:MaxTenuringThreshold=1",
];

fn read_json(path: &PathBuf) -> Result<Value> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("lecture de {} impossible : {e}", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

/// "group:artifact:version[:classifier]" -> chemin maven relatif.
fn maven_path(name: &str) -> Option<String> {
    let (coords, ext) = match name.split_once('@') {
        Some((c, e)) => (c, e),
        None => (name, "jar"),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let (artifact, version) = (parts[1], parts[2]);
    let file = match parts.get(3) {
        Some(cl) => format!("{artifact}-{version}-{cl}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Some(format!("{group}/{artifact}/{version}/{file}"))
}

/// Clé de déduplication : groupe:artefact[:classifieur], sans la version.
fn lib_key(name: &str) -> String {
    let parts: Vec<&str> = name.split('@').next().unwrap_or(name).split(':').collect();
    match parts.len() {
        0..=2 => name.to_string(),
        3 => format!("{}:{}", parts[0], parts[1]),
        _ => format!("{}:{}:{}", parts[0], parts[1], parts[3]),
    }
}

fn sub(s: &str, subs: &HashMap<String, String>) -> String {
    let mut out = s.to_string();
    for (k, v) in subs {
        let needle = format!("${{{k}}}");
        if out.contains(&needle) {
            out = out.replace(&needle, v);
        }
    }
    out
}

fn expand(arg: &Value, subs: &HashMap<String, String>, out: &mut Vec<String>) {
    match arg {
        Value::String(s) => out.push(sub(s, subs)),
        Value::Object(o) => {
            if !rules_allow(o.get("rules")) {
                return;
            }
            match o.get("value") {
                Some(Value::String(s)) => out.push(sub(s, subs)),
                Some(Value::Array(a)) => {
                    for x in a {
                        if let Some(s) = x.as_str() {
                            out.push(sub(s, subs));
                        }
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn arg_list(v: &Value, key: &str) -> Vec<Value> {
    v["arguments"][key].as_array().cloned().unwrap_or_default()
}

pub struct LaunchPlan {
    pub args: Vec<String>,
    pub game_dir: PathBuf,
}

pub fn build(
    s: &Settings,
    session: &Session,
    mc: &str,
    nf: &str,
    server_address: &str,
) -> Result<LaunchPlan> {
    let base = paths::base_dir();
    let nf_id = neoforge_id(nf);
    let parent = read_json(&base.join("versions").join(mc).join(format!("{mc}.json")))?;
    let child = read_json(&base.join("versions").join(&nf_id).join(format!("{nf_id}.json")))?;

    // Classpath : bibliothèques du parent puis de l'enfant (l'enfant l'emporte), puis client.jar.
    let mut order: Vec<String> = Vec::new();
    let mut libs: HashMap<String, PathBuf> = HashMap::new();
    for ver in [&parent, &child] {
        if let Some(arr) = ver["libraries"].as_array() {
            for lib in arr {
                if !rules_allow(lib.get("rules")) {
                    continue;
                }
                let name = lib["name"].as_str().unwrap_or("");
                let rel = lib["downloads"]["artifact"]["path"]
                    .as_str()
                    .map(|p| p.to_string())
                    .or_else(|| maven_path(name));
                if let Some(rel) = rel {
                    let key = lib_key(name);
                    if !libs.contains_key(&key) {
                        order.push(key.clone());
                    }
                    libs.insert(key, base.join("libraries").join(rel));
                }
            }
        }
    }
    let sep = if cfg!(windows) { ";" } else { ":" };
    let mut cp: Vec<String> = order
        .iter()
        .filter_map(|k| libs.get(k))
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    cp.push(
        base.join("versions")
            .join(mc)
            .join(format!("{mc}.jar"))
            .to_string_lossy()
            .to_string(),
    );
    let classpath = cp.join(sep);

    let game_dir = paths::instance_dir();
    std::fs::create_dir_all(&game_dir)?;
    let natives = base.join("natives").join(&nf_id);
    std::fs::create_dir_all(&natives)?;

    let mut subs: HashMap<String, String> = HashMap::new();
    let mut put = |k: &str, v: String| {
        subs.insert(k.to_string(), v);
    };
    put("auth_player_name", session.name.clone());
    put("version_name", nf_id.clone());
    put("game_directory", game_dir.to_string_lossy().to_string());
    put("assets_root", base.join("assets").to_string_lossy().to_string());
    put(
        "assets_index_name",
        parent["assetIndex"]["id"].as_str().unwrap_or("").to_string(),
    );
    put("auth_uuid", session.uuid.clone());
    put("auth_access_token", session.access_token.clone());
    put("clientid", String::new());
    put("auth_xuid", String::new());
    put("user_type", "msa".into());
    put("version_type", "release".into());
    put("natives_directory", natives.to_string_lossy().to_string());
    put("launcher_name", "PokemonUnity".into());
    put("launcher_version", env!("CARGO_PKG_VERSION").into());
    put("classpath", classpath);
    put("classpath_separator", sep.into());
    put("library_directory", base.join("libraries").to_string_lossy().to_string());

    let mut jvm: Vec<String> = Vec::new();
    for a in arg_list(&parent, "jvm").iter().chain(arg_list(&child, "jvm").iter()) {
        expand(a, &subs, &mut jvm);
    }
    if !jvm.iter().any(|a| a == "-cp" || a == "-classpath") {
        jvm.push("-cp".into());
        jvm.push(subs["classpath"].clone());
    }
    let mut game: Vec<String> = Vec::new();
    for a in arg_list(&parent, "game").iter().chain(arg_list(&child, "game").iter()) {
        expand(a, &subs, &mut game);
    }
    let main_class = child["mainClass"]
        .as_str()
        .or_else(|| parent["mainClass"].as_str())
        .ok_or_else(|| anyhow!("mainClass introuvable"))?
        .to_string();

    // Assemblage final
    let mut args: Vec<String> = Vec::new();
    args.push(format!("-Xms{}M", (s.ram_mb / 2).max(1024)));
    args.push(format!("-Xmx{}M", s.ram_mb.max(2048)));
    if s.optimized_jvm {
        args.extend(OPTIMIZED_JVM.iter().map(|x| x.to_string()));
    }
    args.extend(s.jvm_extra.split_whitespace().map(|x| x.to_string()));
    args.extend(jvm);
    args.push(main_class);
    args.extend(game);

    if s.fullscreen {
        args.push("--fullscreen".into());
    } else {
        args.push("--width".into());
        args.push(s.width.to_string());
        args.push("--height".into());
        args.push(s.height.to_string());
    }
    if s.auto_join && !server_address.trim().is_empty() {
        args.push("--quickPlayMultiplayer".into());
        args.push(server_address.trim().to_string());
    }
    let _ = os_name();
    Ok(LaunchPlan { args, game_dir })
}

// ---------------------------------------------------------------------------
// options.txt : applique les réglages graphiques du launcher
// ---------------------------------------------------------------------------

pub fn write_options(s: &Settings) -> Result<()> {
    let dir = paths::instance_dir();
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("options.txt");
    let existing = std::fs::read_to_string(&file).unwrap_or_default();

    let mut wanted: Vec<(&str, String)> = vec![
        ("renderDistance", s.render_distance.to_string()),
        ("simulationDistance", s.simulation_distance.to_string()),
        ("maxFps", s.max_fps.to_string()),
        ("enableVsync", s.vsync.to_string()),
        ("graphicsMode", s.graphics_mode.to_string()),
        ("particles", s.particles.to_string()),
        ("renderClouds", format!("\"{}\"", s.clouds)),
        ("biomeBlendRadius", s.biome_blend.to_string()),
        ("entityDistanceScaling", format!("{:.1}", s.entity_scale)),
        ("mipmapLevels", s.mipmaps.to_string()),
        ("guiScale", s.gui_scale.to_string()),
    ];

    let mut lines: Vec<String> = existing.lines().map(|l| l.to_string()).collect();
    for (key, value) in wanted.drain(..) {
        let prefix = format!("{key}:");
        let new_line = format!("{key}:{value}");
        match lines.iter_mut().find(|l| l.starts_with(&prefix)) {
            Some(l) => *l = new_line,
            None => lines.push(new_line),
        }
    }
    std::fs::write(&file, lines.join("\n") + "\n")?;
    Ok(())
}
