# Pokémon Unity — Launcher

Launcher de bureau (Rust + Tauri 2, interface HTML/CSS/JS aux couleurs d'Unity).

**Ce qu'il fait**
- Connexion au compte Microsoft / Minecraft (flux « code appareil », jeton gardé dans le coffre du système)
- Télécharge et met à jour tout seul : Java 21, Minecraft 1.21.1, NeoForge, puis ton modpack (zip vérifié par SHA-256)
- Bouton unique : TÉLÉCHARGER → METTRE À JOUR → JOUER / REJOINDRE (connexion auto au serveur en option)
- Mémoire RAM réglable (avec recommandation selon le PC), Java optimisé, arguments JVM
- Réglages graphiques (préréglages Performance / Équilibré / Qualité + détail) écrits dans `options.txt` avant chaque lancement
- Actualités et World Boss en cours (vie restante, joueurs, compte à rebours), statut du serveur (joueurs, ping)
- Console avec le journal du jeu, bouton Arrêter, mode maintenance

## 1. Prérequis (une seule fois)

1. Installer Rust : https://rustup.rs
2. Windows : Outils de build Visual Studio (charge « Développement Desktop en C++ ») et WebView2 (déjà présent sous Windows 11)
3. `cargo install tauri-cli --version "^2" --locked`

## 2. Tester / compiler

```
cd launcher
cargo tauri dev        # lance le launcher en mode développement
cargo tauri build      # génère l'installateur (src-tauri/target/release/bundle/)
```

Alternative sans rien installer : pousse le dossier sur GitHub, mets un tag `v0.1.0` (ou lance le workflow « Build launcher » à la main). Les installateurs Windows, macOS et Linux sont créés automatiquement dans une release brouillon (`.github/workflows/build.yml`).

> Le code a été écrit sans pouvoir le compiler (pas d'accès à crates.io dans mon environnement). Si `cargo` signale une erreur, envoie-moi le message tel quel et je le corrige.

## 3. À configurer avant de distribuer

### a) Connexion Microsoft (obligatoire)
Il faut ton propre identifiant d'application :
1. https://portal.azure.com → Microsoft Entra ID → Inscriptions d'applications → Nouvelle inscription
2. Comptes pris en charge : **comptes Microsoft personnels uniquement** (ou « tout annuaire + personnels »)
3. Authentification → Paramètres avancés → « Autoriser les flux clients publics » : **Oui**
4. Copie l'**ID d'application (client)** dans `src-tauri/src/settings.rs` (`DEFAULT_MS_CLIENT_ID`) ou dans Paramètres → Avancé
5. Mojang doit approuver ton application pour l'API Minecraft (formulaire « Minecraft API request » sur help.minecraft.net). Tant que ce n'est pas fait, la connexion peut être refusée.

### b) Fichier d'infos distant (actus, World Boss, version du modpack)
Héberge `remote/launcher.json` (GitHub raw, ton site, un CDN) et mets son URL dans `DEFAULT_REMOTE_URL` (`settings.rs`). Tu modifies ce fichier pour publier une actu, annoncer un boss ou sortir une version ; les joueurs le voient dans les 2 minutes. Pour afficher la vie **réelle** du boss, fais écrire ce fichier par ton serveur (un petit plugin ou script qui met à jour `hp_percent` et `players` toutes les minutes).

### c) Le modpack
```
python tools/make_modpack.py <dossier_instance> 0.1.0 https://ton-site/pokemon-unity-modpack-0.1.0.zip
```
Le dossier contient `mods/` (ton `pokemon_unity-1.0.0.jar`, Cobblemon, Mega Showdown, Kotlin for Forge…), `config/`, `resourcepacks/`, etc. Le script crée le zip et affiche le bloc `modpack` (version, url, sha256) à coller dans `launcher.json`. À chaque nouvelle version : nouveau zip, nouvelle version, c'est tout. Les mises à jour remplacent seulement les fichiers de l'ancienne version, les mondes et `options.txt` sont conservés.

### d) Version de NeoForge
Dans `launcher.json` : `"neoforge": "21.1.265"` doit correspondre à celle de ton mod.

### e) Police et icône
- Dépose `Outfit.ttf` dans `ui/fonts/` (sinon police système).
- Remplace les images de `src-tauri/icons/` par ton logo (`cargo tauri icon mon-logo.png` les régénère toutes).

## 4. Où sont les données du joueur
`%APPDATA%\PokemonUnity` : `instance/` (jeu, mods, mondes), `runtime/` (Java), `versions/` + `libraries/` + `assets/` (Minecraft), `launcher_settings.json`.

## 5. Structure
```
src-tauri/src/
  main.rs      commandes Tauri (jouer, installer, connexion, statut…)
  auth.rs      Microsoft -> Xbox -> XSTS -> Minecraft
  install.rs   Java 21, Minecraft, NeoForge, modpack (téléchargements vérifiés)
  launch.rs    ligne de commande Java + options.txt
  remote.rs    infos distantes + ping du serveur
  settings.rs  paramètres et constantes à remplacer
ui/            interface (index.html, style.css, app.js)
```
Ouvrir `ui/index.html` dans un navigateur affiche l'interface avec des données factices, pratique pour retoucher le design.
