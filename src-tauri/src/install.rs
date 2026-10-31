//! Installation : Java 21, Minecraft 1.21.1, NeoForge, modpack.
use crate::paths;
use crate::remote::ModpackInfo;
use anyhow::{anyhow, bail, Context, Result};
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

// ---------------------------------------------------------------------------
// Progression
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub stage: String,
    pub label: String,
    pub current: u64,
    pub total: u64,
}

pub type Emit = Arc<dyn Fn(Progress) + Send + Sync>;

fn emit(e: &Emit, stage: &str, label: &str, current: u64, total: u64) {
    e(Progress {
        stage: stage.into(),
        label: label.into(),
        current,
        total,
    });
}

// ---------------------------------------------------------------------------
// État d'installation
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Installed {
    pub modpack_version: String,
    pub files: Vec<String>,
}

impl Installed {
    pub fn load() -> Self {
        std::fs::read_to_string(paths::installed_file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(paths::base_dir())?;
        std::fs::write(paths::installed_file(), serde_json::to_string(self)?)?;
        Ok(())
    }
}

pub fn neoforge_id(nf: &str) -> String {
    format!("neoforge-{nf}")
}

pub fn minecraft_ready(mc: &str) -> bool {
    let d = paths::base_dir().join("versions").join(mc);
    d.join(format!("{mc}.json")).exists() && d.join(format!("{mc}.jar")).exists()
}

pub fn neoforge_ready(nf: &str) -> bool {
    let id = neoforge_id(nf);
    paths::base_dir()
        .join("versions")
        .join(&id)
        .join(format!("{id}.json"))
        .exists()
}

// ---------------------------------------------------------------------------
// Téléchargement avec vérification d'intégrité
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Check {
    None,
    Sha1(String),
    Sha256(String),
}

enum Hasher {
    None,
    Sha1(Sha1),
    Sha256(Sha256),
}

impl Hasher {
    fn new(c: &Check) -> Self {
        match c {
            Check::None => Hasher::None,
            Check::Sha1(_) => Hasher::Sha1(Sha1::new()),
            Check::Sha256(_) => Hasher::Sha256(Sha256::new()),
        }
    }
    fn update(&mut self, data: &[u8]) {
        match self {
            Hasher::None => {}
            Hasher::Sha1(h) => h.update(data),
            Hasher::Sha256(h) => h.update(data),
        }
    }
    fn finish(self) -> String {
        match self {
            Hasher::None => String::new(),
            Hasher::Sha1(h) => hex::encode(h.finalize()),
            Hasher::Sha256(h) => hex::encode(h.finalize()),
        }
    }
}

fn expected(c: &Check) -> Option<&str> {
    match c {
        Check::None => None,
        Check::Sha1(s) | Check::Sha256(s) => {
            if s.is_empty() {
                None
            } else {
                Some(s.as_str())
            }
        }
    }
}

async fn file_hash(path: &Path, check: &Check) -> Result<String> {
    let path = path.to_path_buf();
    let check = check.clone();
    tokio::task::spawn_blocking(move || -> Result<String> {
        use std::io::Read;
        let mut f = std::fs::File::open(path)?;
        let mut h = Hasher::new(&check);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(h.finish())
    })
    .await?
}

/// Télécharge `url` vers `dest` (3 essais). Saute le fichier s'il existe déjà et est valide.
pub async fn download(client: &reqwest::Client, url: &str, dest: &Path, check: &Check) -> Result<()> {
    if dest.exists() {
        match expected(check) {
            Some(want) => {
                if file_hash(dest, check).await?.eq_ignore_ascii_case(want) {
                    return Ok(());
                }
            }
            None => return Ok(()),
        }
    }
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut last: Option<anyhow::Error> = None;
    for _ in 0..3 {
        match download_once(client, url, dest, check).await {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| anyhow!("téléchargement échoué")))
        .with_context(|| format!("téléchargement de {url}"))
}

async fn download_once(client: &reqwest::Client, url: &str, dest: &Path, check: &Check) -> Result<()> {
    let tmp = PathBuf::from(format!("{}.part", dest.display()));
    let resp = client.get(url).send().await?.error_for_status()?;
    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut hasher = Hasher::new(check);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    let got = hasher.finish();
    if let Some(want) = expected(check) {
        if !got.eq_ignore_ascii_case(want) {
            let _ = tokio::fs::remove_file(&tmp).await;
            bail!("somme de contrôle incorrecte (attendu {want}, reçu {got})");
        }
    }
    tokio::fs::rename(&tmp, dest).await?;
    Ok(())
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<Value> {
    Ok(client.get(url).send().await?.error_for_status()?.json().await?)
}

struct Job {
    url: String,
    dest: PathBuf,
    sha1: String,
}

async fn download_many(
    client: &reqwest::Client,
    jobs: Vec<Job>,
    e: &Emit,
    stage: &str,
    label: &str,
) -> Result<()> {
    let total = jobs.len() as u64;
    let done = Arc::new(AtomicU64::new(0));
    emit(e, stage, label, 0, total);
    let results: Vec<Result<()>> = stream::iter(jobs)
        .map(|job| {
            let client = client.clone();
            let done = done.clone();
            let e = e.clone();
            let stage = stage.to_string();
            let label = label.to_string();
            async move {
                let check = if job.sha1.is_empty() {
                    Check::None
                } else {
                    Check::Sha1(job.sha1.clone())
                };
                let r = download(&client, &job.url, &job.dest, &check).await;
                let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                if n % 8 == 0 || n == total {
                    emit(&e, &stage, &label, n, total);
                }
                r
            }
        })
        .buffer_unordered(16)
        .collect()
        .await;
    for r in results {
        r?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Règles Mojang (os / features)
// ---------------------------------------------------------------------------

pub fn os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

pub fn rules_allow(rules: Option<&Value>) -> bool {
    let rules = match rules.and_then(|r| r.as_array()) {
        Some(r) if !r.is_empty() => r,
        _ => return true,
    };
    let mut allowed = false;
    for rule in rules {
        let mut matches = rule.get("features").is_none();
        if let Some(os) = rule.get("os") {
            if let Some(name) = os["name"].as_str() {
                if name != os_name() {
                    matches = false;
                }
            }
            if let Some(arch) = os["arch"].as_str() {
                if arch == "x86" && cfg!(target_pointer_width = "64") {
                    matches = false;
                }
            }
        }
        if matches {
            allowed = rule["action"].as_str() == Some("allow");
        }
    }
    allowed
}

// ---------------------------------------------------------------------------
// Minecraft vanilla
// ---------------------------------------------------------------------------

pub async fn install_minecraft(client: &reqwest::Client, mc: &str, e: &Emit) -> Result<()> {
    let base = paths::base_dir();
    emit(e, "minecraft", "Récupération de la version Minecraft", 0, 0);

    let manifest = get_json(
        client,
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
    )
    .await?;
    let entry = manifest["versions"]
        .as_array()
        .and_then(|a| a.iter().find(|v| v["id"].as_str() == Some(mc)))
        .ok_or_else(|| anyhow!("version Minecraft {mc} introuvable chez Mojang"))?;
    let vurl = entry["url"].as_str().ok_or_else(|| anyhow!("url de version manquante"))?;
    let vsha = entry["sha1"].as_str().unwrap_or("");

    let vdir = base.join("versions").join(mc);
    let vjson_path = vdir.join(format!("{mc}.json"));
    download(
        client,
        vurl,
        &vjson_path,
        &if vsha.is_empty() { Check::None } else { Check::Sha1(vsha.into()) },
    )
    .await?;
    let v: Value = serde_json::from_str(&tokio::fs::read_to_string(&vjson_path).await?)?;

    // Client
    let client_dl = &v["downloads"]["client"];
    download(
        client,
        client_dl["url"].as_str().ok_or_else(|| anyhow!("client.jar manquant"))?,
        &vdir.join(format!("{mc}.jar")),
        &Check::Sha1(client_dl["sha1"].as_str().unwrap_or("").into()),
    )
    .await?;

    // Bibliothèques
    let mut jobs = Vec::new();
    if let Some(libs) = v["libraries"].as_array() {
        for lib in libs {
            if !rules_allow(lib.get("rules")) {
                continue;
            }
            if let Some(a) = lib["downloads"]["artifact"].as_object() {
                let path = a.get("path").and_then(|p| p.as_str()).unwrap_or("");
                let url = a.get("url").and_then(|p| p.as_str()).unwrap_or("");
                if path.is_empty() || url.is_empty() {
                    continue;
                }
                jobs.push(Job {
                    url: url.into(),
                    dest: base.join("libraries").join(path),
                    sha1: a.get("sha1").and_then(|p| p.as_str()).unwrap_or("").into(),
                });
            }
        }
    }
    download_many(client, jobs, e, "minecraft", "Bibliothèques Minecraft").await?;

    // Ressources (sons, langues…)
    let ai = &v["assetIndex"];
    let aid = ai["id"].as_str().ok_or_else(|| anyhow!("assetIndex manquant"))?;
    let index_path = base.join("assets").join("indexes").join(format!("{aid}.json"));
    download(
        client,
        ai["url"].as_str().ok_or_else(|| anyhow!("url assetIndex manquante"))?,
        &index_path,
        &Check::Sha1(ai["sha1"].as_str().unwrap_or("").into()),
    )
    .await?;
    let index: Value = serde_json::from_str(&tokio::fs::read_to_string(&index_path).await?)?;
    let mut jobs = Vec::new();
    if let Some(objs) = index["objects"].as_object() {
        for (_, obj) in objs {
            let hash = obj["hash"].as_str().unwrap_or("");
            if hash.len() < 2 {
                continue;
            }
            let prefix = &hash[..2];
            jobs.push(Job {
                url: format!("https://resources.download.minecraft.net/{prefix}/{hash}"),
                dest: base.join("assets").join("objects").join(prefix).join(hash),
                sha1: hash.into(),
            });
        }
    }
    download_many(client, jobs, e, "minecraft", "Ressources du jeu").await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Java
// ---------------------------------------------------------------------------

fn java_exe_name() -> &'static str {
    if cfg!(windows) {
        "java.exe"
    } else {
        "java"
    }
}

fn find_java(dir: &Path) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(f) = find_java(&p) {
                return Some(f);
            }
        } else if p.file_name().and_then(|n| n.to_str()) == Some(java_exe_name())
            && p.parent().and_then(|b| b.file_name()).and_then(|n| n.to_str()) == Some("bin")
        {
            return Some(p);
        }
    }
    None
}

/// Version majeure de Java (21, 17…) en lançant `java -version`.
pub async fn java_major(java: &Path) -> Option<u32> {
    let mut cmd = tokio::process::Command::new(java);
    cmd.arg("-version");
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let out = cmd.output().await.ok()?;
    let text = String::from_utf8_lossy(&out.stderr).to_string()
        + &String::from_utf8_lossy(&out.stdout);
    let start = text.find('"')? + 1;
    let rest = &text[start..];
    let end = rest.find('"')?;
    let ver = &rest[..end];
    let mut parts = ver.split(|c: char| c == '.' || c == '-' || c == '+' || c == '_');
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

pub async fn ensure_java(client: &reqwest::Client, custom: &str, e: &Emit) -> Result<PathBuf> {
    // 1. Chemin choisi par l'utilisateur
    if !custom.trim().is_empty() {
        let p = PathBuf::from(custom.trim());
        if p.exists() {
            return Ok(p);
        }
        bail!("Le chemin Java indiqué n'existe pas : {custom}");
    }
    // 2. Java embarqué déjà téléchargé
    let rt = paths::runtime_dir().join("java21");
    if let Some(j) = find_java(&rt) {
        return Ok(j);
    }
    // 3. Java 21+ du système
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let p = PathBuf::from(home).join("bin").join(java_exe_name());
        if p.exists() && java_major(&p).await.unwrap_or(0) >= 21 {
            return Ok(p);
        }
    }
    let sys = PathBuf::from("java");
    if java_major(&sys).await.unwrap_or(0) >= 21 {
        return Ok(sys);
    }
    // 4. Téléchargement d'Eclipse Temurin 21
    emit(e, "java", "Téléchargement de Java 21", 0, 0);
    let os = match os_name() {
        "windows" => "windows",
        "osx" => "mac",
        _ => "linux",
    };
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
    let url = format!(
        "https://api.adoptium.net/v3/binary/latest/21/ga/{os}/{arch}/jre/hotspot/normal/eclipse"
    );
    let archive = paths::cache_dir().join(if cfg!(windows) { "java21.zip" } else { "java21.tar.gz" });
    let _ = tokio::fs::remove_file(&archive).await;
    download(client, &url, &archive, &Check::None).await?;
    emit(e, "java", "Installation de Java 21", 0, 0);
    let rt2 = rt.clone();
    let archive2 = archive.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let _ = std::fs::remove_dir_all(&rt2);
        std::fs::create_dir_all(&rt2)?;
        let f = std::fs::File::open(&archive2)?;
        if cfg!(windows) {
            zip::ZipArchive::new(f)?.extract(&rt2)?;
        } else {
            tar::Archive::new(flate2::read::GzDecoder::new(f)).unpack(&rt2)?;
        }
        Ok(())
    })
    .await??;
    let _ = tokio::fs::remove_file(&archive).await;
    find_java(&rt).ok_or_else(|| anyhow!("Java 21 installé mais introuvable"))
}

// ---------------------------------------------------------------------------
// NeoForge (via l'installateur officiel en mode client)
// ---------------------------------------------------------------------------

pub async fn install_neoforge(client: &reqwest::Client, java: &Path, nf: &str, e: &Emit) -> Result<()> {
    if neoforge_ready(nf) {
        return Ok(());
    }
    let base = paths::base_dir();
    emit(e, "neoforge", "Téléchargement de NeoForge", 0, 0);
    let installer = paths::cache_dir().join(format!("neoforge-{nf}-installer.jar"));
    let url = format!(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/{nf}/neoforge-{nf}-installer.jar"
    );
    download(client, &url, &installer, &Check::None).await?;

    // L'installateur exige un launcher_profiles.json dans le dossier cible.
    let profiles = base.join("launcher_profiles.json");
    if !profiles.exists() {
        tokio::fs::write(&profiles, r#"{"profiles":{}}"#).await?;
    }

    emit(e, "neoforge", "Installation de NeoForge (peut prendre une minute)", 0, 0);
    let mut cmd = tokio::process::Command::new(java);
    cmd.arg("-jar").arg(&installer).arg("--installClient").arg(&base);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let out = cmd.output().await.context("lancement de l'installateur NeoForge")?;
    if !out.status.success() || !neoforge_ready(nf) {
        let log = String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = log.lines().rev().take(12).collect();
        bail!(
            "L'installation de NeoForge a échoué :\n{}",
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Modpack (archive zip : mods/, config/, resourcepacks/…)
// ---------------------------------------------------------------------------

/// Fichiers que l'archive ne doit jamais écraser s'ils existent déjà.
const KEEP_IF_EXISTS: [&str; 2] = ["options.txt", "servers.dat"];

pub async fn install_modpack(client: &reqwest::Client, pack: &ModpackInfo, e: &Emit) -> Result<()> {
    if pack.url.is_empty() {
        bail!("Aucune archive de modpack n'est publiée pour l'instant.");
    }
    emit(e, "modpack", "Téléchargement du modpack", 0, pack.size);
    let archive = paths::cache_dir().join(format!("modpack-{}.zip", pack.version));
    let check = if pack.sha256.is_empty() {
        Check::None
    } else {
        Check::Sha256(pack.sha256.clone())
    };
    download(client, &pack.url, &archive, &check).await?;

    emit(e, "modpack", "Installation du modpack", 0, 0);
    let instance = paths::instance_dir();
    let old = Installed::load();
    let version = pack.version.clone();
    let e2 = e.clone();
    let files = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
        std::fs::create_dir_all(&instance)?;
        // Supprime les fichiers de l'ancienne version (mais pas les saves ni les options).
        for f in &old.files {
            let _ = std::fs::remove_file(instance.join(f));
        }
        let file = std::fs::File::open(&archive)?;
        let mut zip = zip::ZipArchive::new(file)?;
        let total = zip.len() as u64;
        let mut written = Vec::new();
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let rel = match entry.enclosed_name() {
                Some(p) => p,
                None => continue, // chemin dangereux ignoré
            };
            let out = instance.join(&rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&out)?;
                continue;
            }
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if KEEP_IF_EXISTS.contains(&rel_str.as_str()) && out.exists() {
                continue;
            }
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut f = std::fs::File::create(&out)?;
            std::io::copy(&mut entry, &mut f)?;
            written.push(rel_str);
            if i % 25 == 0 {
                emit(&e2, "modpack", "Installation du modpack", i as u64, total);
            }
        }
        Ok(written)
    })
    .await??;

    Installed {
        modpack_version: version,
        files,
    }
    .save()?;
    emit(e, "modpack", "Modpack installé", 1, 1);
    Ok(())
}
