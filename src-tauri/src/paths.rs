use std::path::PathBuf;

/// Dossier racine des données du launcher (%APPDATA%\PokemonUnity sous Windows).
pub fn base_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PokemonUnity")
}

/// Dossier du jeu (mods, config, saves, options.txt).
pub fn instance_dir() -> PathBuf {
    base_dir().join("instance")
}

pub fn runtime_dir() -> PathBuf {
    base_dir().join("runtime")
}

pub fn cache_dir() -> PathBuf {
    base_dir().join("cache")
}

pub fn settings_file() -> PathBuf {
    base_dir().join("launcher_settings.json")
}

pub fn account_file() -> PathBuf {
    base_dir().join("account.json")
}

pub fn installed_file() -> PathBuf {
    base_dir().join("installed.json")
}

pub fn remote_cache_file() -> PathBuf {
    cache_dir().join("launcher_remote.json")
}
