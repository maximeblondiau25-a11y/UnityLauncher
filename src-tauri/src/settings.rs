use crate::paths;
use serde::{Deserialize, Serialize};

/// URL du fichier JSON distant (actus, World Boss, version du modpack).
/// À remplacer par l'URL réelle (GitHub raw, CDN, ton site…).
pub const DEFAULT_REMOTE_URL: &str =
    "https://raw.githubusercontent.com/TON-COMPTE/pokemon-unity-launcher/main/remote/launcher.json";

/// Identifiant d'application Azure (client id) pour la connexion Microsoft.
/// Voir le README : il faut enregistrer une application et demander l'accès à l'API Minecraft.
pub const DEFAULT_MS_CLIENT_ID: &str = "REMPLACE-PAR-TON-CLIENT-ID-AZURE";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Mémoire et Java
    pub ram_mb: u32,
    pub java_path: String,
    pub optimized_jvm: bool,
    pub jvm_extra: String,

    // Graphismes (écrits dans options.txt avant chaque lancement)
    pub preset: String,
    pub render_distance: u32,
    pub simulation_distance: u32,
    pub max_fps: u32,
    pub vsync: bool,
    pub graphics_mode: u32, // 0 rapide, 1 détaillé, 2 spectaculaire
    pub particles: u32,     // 0 tout, 1 réduit, 2 minimal
    pub clouds: String,     // "true" | "fast" | "false"
    pub biome_blend: u32,
    pub entity_scale: f32,
    pub mipmaps: u32,
    pub gui_scale: u32, // 0 = auto

    // Fenêtre et comportement
    pub fullscreen: bool,
    pub width: u32,
    pub height: u32,
    pub auto_join: bool,
    pub close_on_launch: bool,

    // Avancé
    pub remote_url: String,
    pub ms_client_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ram_mb: 6144,
            java_path: String::new(),
            optimized_jvm: true,
            jvm_extra: String::new(),
            preset: "balanced".into(),
            render_distance: 12,
            simulation_distance: 10,
            max_fps: 144,
            vsync: false,
            graphics_mode: 1,
            particles: 1,
            clouds: "fast".into(),
            biome_blend: 3,
            entity_scale: 1.0,
            mipmaps: 2,
            gui_scale: 0,
            fullscreen: false,
            width: 1280,
            height: 720,
            auto_join: false,
            close_on_launch: false,
            remote_url: DEFAULT_REMOTE_URL.into(),
            ms_client_id: DEFAULT_MS_CLIENT_ID.into(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(paths::settings_file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(paths::base_dir())?;
        std::fs::write(paths::settings_file(), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
