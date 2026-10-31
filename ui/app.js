/* Pokémon Unity — logique de l'interface du launcher. */
"use strict";

// ---------------------------------------------------------------------------
// Pont Tauri (avec données factices quand on ouvre la page dans un navigateur)
// ---------------------------------------------------------------------------
const T = window.__TAURI__;
const inTauri = !!T;
const listen = inTauri ? T.event.listen : () => Promise.resolve(() => {});
const win = inTauri ? T.window.getCurrentWindow() : null;

const MOCK_REMOTE = {
  minecraft: "1.21.1", neoforge: "21.1.265",
  modpack: { version: "0.3.0", url: "x", sha256: "", size: 0, changelog: "" },
  server: { name: "Pokémon Unity", address: "play.exemple.net" },
  maintenance: "",
  news: [
    { id: "1", title: "La brèche des Plaines Venteuses s'ouvre", date: "2026-10-30", tag: "ÉVÉNEMENT", summary: "La Team Origin a été repérée près de la tour fédérale. Les dresseurs de niveau 20 et plus sont invités à former un groupe de 4.", url: "" },
    { id: "2", title: "Mise à jour 0.3.0 : Stades d'Origine", date: "2026-10-28", tag: "MISE À JOUR", summary: "Les Pokémon peuvent désormais dépasser le niveau 100. Découvre le premier Stade d'Origine et l'emplacement de talent ascendant.", url: "" },
    { id: "3", title: "Ouverture de la bêta fermée", date: "2026-10-20", tag: "ANNONCE", summary: "Les premiers dresseurs entrent à Unity ce week-end. Merci de signaler tout problème sur le Discord.", url: "" },
  ],
  bosses: [
    { id: "b1", name: "Dracaufeu corrompu", form: "Anomalie", level: 62, status: "active", hp_percent: 64.2, players: 183, ends_at: new Date(Date.now() + 3 * 3600e3 + 12 * 60e3).toISOString(), description: "Une brèche s'est ouverte près de la tour fédérale. Toute la région participe au combat." },
    { id: "b2", name: "???", form: "Signal inconnu", level: 100, status: "upcoming", hp_percent: 100, players: 0, ends_at: new Date(Date.now() + 2 * 86400e3).toISOString(), description: "L'Expédition capte une énergie inhabituelle. Les détails seront communiqués bientôt." },
    { id: "b3", name: "Tortank corrompu", form: "Anomalie", level: 55, status: "defeated", hp_percent: 0, players: 412, ends_at: "", description: "Vaincu par la communauté lors de la brèche de la semaine dernière." },
  ],
};
let mockSettings = null;
const mockInvoke = async (cmd, args) => {
  await new Promise(r => setTimeout(r, 120));
  switch (cmd) {
    case "get_settings": return mockSettings || (mockSettings = {
      ram_mb: 6144, java_path: "", optimized_jvm: true, jvm_extra: "", preset: "balanced",
      render_distance: 12, simulation_distance: 10, max_fps: 144, vsync: false, graphics_mode: 1,
      particles: 1, clouds: "fast", biome_blend: 3, entity_scale: 1.0, mipmaps: 2, gui_scale: 0,
      fullscreen: false, width: 1280, height: 720, auto_join: false, close_on_launch: false,
      remote_url: "https://exemple.net/launcher.json", ms_client_id: "REMPLACE-PAR-TON-CLIENT-ID-AZURE" });
    case "save_settings": mockSettings = args.settings; return null;
    case "system_info": return { total_ram_mb: 16384, os: "windows", data_dir: "C:\\Users\\Hellyhsia\\AppData\\Roaming\\PokemonUnity", version: "0.1.0" };
    case "get_account": return { name: "Hellyhsia", uuid: "00000000000000000000000000000000" };
    case "fetch_remote": return { info: MOCK_REMOTE, offline: false };
    case "server_status": return { online: true, players: 248, max: 500, ping_ms: 32, version: "1.21.1", motd: "" };
    case "install_state": return { installed: { modpack_version: "0.2.0", files: [] }, minecraft_ready: true, neoforge_ready: true, running: false };
    default: return null;
  }
};
const invoke = inTauri ? T.core.invoke : mockInvoke;

// ---------------------------------------------------------------------------
// Utilitaires
// ---------------------------------------------------------------------------
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const esc = s => String(s ?? "").replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const state = { settings: null, sys: null, remote: null, offline: false, account: null, install: null, busy: false, running: false, status: null };

function fmtDate(iso) {
  const d = new Date(iso);
  return isNaN(d) ? "" : d.toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric" });
}
function countdown(iso) {
  if (!iso) return "";
  const ms = new Date(iso) - Date.now();
  if (isNaN(ms)) return "";
  if (ms <= 0) return "00:00:00";
  const s = Math.floor(ms / 1000), d = Math.floor(s / 86400);
  const h = String(Math.floor(s % 86400 / 3600)).padStart(2, "0"), m = String(Math.floor(s % 3600 / 60)).padStart(2, "0"), sec = String(s % 60).padStart(2, "0");
  return d > 0 ? `${d} j ${h} h` : `${h}:${m}:${sec}`;
}
function tagClass(tag) {
  const t = (tag || "").toUpperCase();
  if (t.includes("ÉVÉN") || t.includes("EVEN")) return "violet";
  if (t.includes("MISE")) return "green";
  if (t.includes("MAINT")) return "amber";
  if (t.includes("URGENT")) return "red";
  return "";
}
function go(url) { if (url && inTauri) invoke("open_url", { url }); else if (url) window.open(url, "_blank"); }

// ---------------------------------------------------------------------------
// Étoiles du décor
// ---------------------------------------------------------------------------
(function stars() {
  const g = $("#stars"); let out = "";
  let seed = 7; const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  for (let i = 0; i < 110; i++) out += `<circle cx="${(rnd() * 1200).toFixed(0)}" cy="${(rnd() * 620).toFixed(0)}" r="${(0.5 + rnd() * 1.1).toFixed(1)}" opacity="${(0.25 + rnd() * 0.65).toFixed(2)}"/>`;
  g.innerHTML = out;
})();

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------
function showPage(name) {
  $$(".page").forEach(p => p.classList.toggle("active", p.id === "page-" + name));
  $$(".nav-btn").forEach(b => b.classList.toggle("active", b.dataset.page === name));
}
$$(".nav-btn").forEach(b => b.addEventListener("click", () => showPage(b.dataset.page)));
$$("#subnav .sub").forEach(b => b.addEventListener("click", () => {
  $$("#subnav .sub").forEach(x => x.classList.toggle("active", x === b));
  $$(".panel").forEach(p => p.classList.toggle("active", p.id === "tab-" + b.dataset.tab));
}));

if (win) {
  $("#tb-min").onclick = () => win.minimize();
  $("#tb-max").onclick = () => win.toggleMaximize();
  $("#tb-close").onclick = () => win.close();
} else {
  $$(".tb-btn").forEach(b => b.disabled = true);
}

// ---------------------------------------------------------------------------
// Compte
// ---------------------------------------------------------------------------
function avatar(uuid) { return uuid ? `https://mc-heads.net/avatar/${uuid}/72` : "data:image/svg+xml;utf8," + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="72" height="72"><rect width="72" height="72" fill="#0F1530"/><circle cx="36" cy="28" r="12" fill="#3A4A86"/><rect x="16" y="46" width="40" height="20" rx="10" fill="#3A4A86"/></svg>'); }

function renderAccount() {
  const a = state.account;
  $("#acc-name").textContent = a ? a.name : "Non connecté";
  $("#acc-sub").textContent = a ? "Compte Microsoft" : "Clique pour te connecter";
  $("#acc-avatar").src = avatar(a?.uuid);
  $("#acc-avatar-big").src = avatar(a?.uuid);
  $("#acc-big-name").textContent = a ? a.name : "Non connecté";
  $("#acc-big-uuid").textContent = a ? a.uuid : "Connecte ton compte Microsoft pour jouer.";
  $("#login-btn").hidden = !!a;
  $("#logout-btn").hidden = !a;
  renderPlay();
}
$("#account-chip").onclick = () => { if (state.account) { showPage("settings"); } else openLogin(); };
$("#login-btn").onclick = openLogin;
$("#logout-btn").onclick = async () => { await invoke("logout"); state.account = null; renderAccount(); };

function openLogin() {
  const modal = $("#login-modal"), body = $("#lm-body");
  modal.hidden = false;
  body.innerHTML = `<p class="hint" style="margin-top:0">Une page Microsoft va s'ouvrir dans ton navigateur. Entre le code affiché ici pour autoriser le launcher.</p>
    <div class="row-btns"><button class="btn primary" id="lm-go">Continuer</button></div>`;
  $("#lm-go").onclick = async () => {
    body.innerHTML = `<p class="hint" style="margin-top:0"><span class="spin"></span>Demande du code…</p>`;
    try {
      const c = await invoke("login_begin");
      body.innerHTML = `<p class="hint" style="margin-top:0">Ouvre <b>${esc(c.verification_uri)}</b> (déjà ouvert) et saisis :</p>
        <div class="code">${esc(c.user_code)}</div>
        <p class="hint"><span class="spin"></span>En attente de ta validation…</p>`;
    } catch (e) { body.innerHTML = `<div class="errorbox">${esc(e)}</div>`; }
  };
}
$("#lm-close").onclick = () => { $("#login-modal").hidden = true; };
listen("login-done", ev => { state.account = ev.payload; $("#login-modal").hidden = true; renderAccount(); });
listen("login-error", ev => { $("#lm-body").innerHTML = `<div class="errorbox">${esc(ev.payload)}</div>`; });

// ---------------------------------------------------------------------------
// Bouton principal
// ---------------------------------------------------------------------------
function needsInstall() {
  const i = state.install, r = state.remote;
  if (!i || !r) return false;
  return !i.minecraft_ready || !i.neoforge_ready || !i.installed.modpack_version;
}
function needsUpdate() {
  const i = state.install, r = state.remote;
  return !!(i && r && i.installed.modpack_version && i.installed.modpack_version !== r.modpack.version);
}
function renderPlay() {
  const btn = $("#play-btn"), label = $("#play-label"), sub = $("#play-sub");
  const r = state.remote;
  btn.classList.remove("danger"); btn.disabled = false; sub.textContent = "";
  const maint = r && r.maintenance;
  $("#maint").hidden = !maint; if (maint) $("#maint").textContent = "Maintenance : " + r.maintenance;

  if (state.running) { label.textContent = "ARRÊTER LE JEU"; sub.textContent = "Minecraft est en cours d'exécution"; btn.classList.add("danger"); return; }
  if (state.busy) { label.textContent = "EN COURS…"; btn.disabled = true; return; }
  if (!r) { label.textContent = "HORS LIGNE"; sub.textContent = "Infos du launcher indisponibles"; btn.disabled = true; return; }
  if (maint) { label.textContent = "MAINTENANCE"; btn.disabled = true; return; }
  if (!state.account) { label.textContent = "SE CONNECTER"; sub.textContent = "Compte Microsoft requis"; return; }
  if (needsInstall()) { label.textContent = "TÉLÉCHARGER"; sub.textContent = "Installation du modpack " + r.modpack.version; return; }
  if (needsUpdate()) { label.textContent = "METTRE À JOUR"; sub.textContent = state.install.installed.modpack_version + " → " + r.modpack.version; return; }
  label.textContent = state.settings?.auto_join ? "REJOINDRE" : "JOUER";
  sub.textContent = state.settings?.auto_join ? (r.server.name || "Serveur principal") : "Modpack " + r.modpack.version;
}

$("#play-btn").onclick = async () => {
  hideError();
  if (state.running) { await invoke("stop_game"); return; }
  if (!state.account) { openLogin(); return; }
  state.busy = true; renderPlay();
  showProgress({ label: "Préparation…", current: 0, total: 0 });
  try {
    if (needsInstall() || needsUpdate()) {
      await invoke("install_or_update", { info: state.remote });
      await refreshInstall();
      toast("Installation terminée.");
    } else {
      await invoke("play", { info: state.remote });
    }
  } catch (e) { showError(e); }
  state.busy = false; hideProgress(); renderPlay();
};

function showError(msg) { const b = $("#errorbox"); b.hidden = false; b.textContent = msg; }
function hideError() { $("#errorbox").hidden = true; }
function showProgress(p) {
  $("#progress").hidden = false;
  $("#pg-label").textContent = p.label || "";
  const fill = $("#pg-fill");
  if (p.total > 0) { fill.classList.remove("indet"); fill.style.width = Math.min(100, p.current / p.total * 100) + "%"; $("#pg-pct").textContent = Math.floor(p.current / p.total * 100) + " %"; }
  else { fill.classList.add("indet"); fill.style.width = ""; $("#pg-pct").textContent = ""; }
}
function hideProgress() { $("#progress").hidden = true; }
function toast(t) { $("#pg-label").textContent = t; }

listen("progress", ev => { if (state.busy) showProgress(ev.payload); });
listen("game-started", () => { state.running = true; state.busy = false; hideProgress(); renderPlay(); appendLog("— Jeu lancé —"); });
listen("game-exit", ev => { state.running = false; renderPlay(); appendLog(`— Jeu fermé (code ${ev.payload ?? "?"}) —`); if (ev.payload && ev.payload !== 0 && ev.payload !== -1) { showError("Le jeu s'est arrêté avec une erreur (code " + ev.payload + "). Consulte l'onglet Console."); } });

$("#folder-btn").onclick = () => invoke("open_data_folder");

// ---------------------------------------------------------------------------
// Console
// ---------------------------------------------------------------------------
const logLines = [];
function appendLog(line) {
  logLines.push(line); if (logLines.length > 3000) logLines.shift();
  const c = $("#console");
  if (logLines.length === 1) c.textContent = "";
  const atBottom = c.scrollTop + c.clientHeight >= c.scrollHeight - 30;
  c.textContent += line + "\n";
  if (atBottom) c.scrollTop = c.scrollHeight;
}
listen("game-log", ev => appendLog(ev.payload));
$("#log-clear").onclick = () => { logLines.length = 0; $("#console").textContent = ""; };
$("#log-copy").onclick = () => navigator.clipboard?.writeText(logLines.join("\n"));

// ---------------------------------------------------------------------------
// Données distantes : actus, World Boss, serveur
// ---------------------------------------------------------------------------
function renderRemote() {
  const r = state.remote; if (!r) { return; }
  $("#hero-eyebrow").textContent = `MODPACK COBBLEMON · MINECRAFT ${r.minecraft || "1.21.1"}`;
  $("#meta-pack").textContent = r.modpack.version || "—";
  $("#meta-mc").textContent = r.minecraft || "—";
  $("#srv-name").textContent = r.server.name || "Serveur";
  $("#offline-note").textContent = state.offline ? "Hors ligne : dernières actualités connues" : "";

  // Actus
  const news = r.news || [];
  $("#news-badge").hidden = !news.length; $("#news-badge").textContent = news.length;
  $("#news-list").innerHTML = news.length ? news.map(n => `
    <article class="news-item"><div class="top"><span class="tag ${tagClass(n.tag)}">${esc(n.tag || "INFO")}</span><span class="muted">${esc(fmtDate(n.date))}</span></div>
      <h3>${esc(n.title)}</h3><p>${esc(n.summary)}</p>
      ${n.url ? `<div class="more"><button class="btn small" data-url="${esc(n.url)}">Lire la suite</button></div>` : ""}</article>`).join("")
    : `<div class="empty">Aucune actualité pour le moment.</div>`;
  $("#news-mini").innerHTML = news.length ? news.slice(0, 2).map(n => `<button data-nav="news"><div class="nt">${esc(n.title)}</div><div class="nd">${esc(fmtDate(n.date))}</div></button>`).join("") : `<div class="muted">Rien de nouveau.</div>`;

  renderBosses();
  renderPlay();
  $("#meta-ram").textContent = state.settings ? (state.settings.ram_mb / 1024).toFixed(state.settings.ram_mb % 1024 ? 1 : 0) + " Go" : "—";
}

function renderBosses() {
  const r = state.remote; if (!r) return;
  const bosses = r.bosses || [];
  const active = bosses.find(b => b.status === "active");
  $("#boss-badge").hidden = !active;
  const label = { active: ["EN COURS", "violet"], upcoming: ["À VENIR", "amber"], defeated: ["VAINCU", "green"] };
  $("#boss-list").innerHTML = bosses.length ? bosses.map(b => {
    const [txt, cls] = label[b.status] || ["", ""];
    return `<article class="boss-item ${b.status === "active" ? "active" : ""}">
      <div class="boss-head"><div><span class="tag ${cls}">${txt}</span><h3 style="margin-top:10px">${esc(b.name)} <span class="muted" style="font-weight:500">· ${esc(b.form)} · Niv. ${b.level}</span></h3></div>
      ${b.status !== "defeated" && b.ends_at ? `<div class="boss-time" data-end="${esc(b.ends_at)}">${countdown(b.ends_at)}</div>` : ""}</div>
      <p>${esc(b.description)}</p>
      ${b.status === "active" ? `<div class="hp"><div style="width:${Math.max(0, Math.min(100, b.hp_percent))}%"></div></div><div class="hp-row"><span>${b.hp_percent.toFixed(1)} % de vie restante</span><span>${b.players} dresseurs engagés</span></div>` : ""}
      ${b.status === "defeated" ? `<div class="hp-row"><span>${b.players} dresseurs ont participé</span></div>` : ""}</article>`;
  }).join("") : `<div class="empty">Aucun World Boss annoncé.</div>`;

  const mini = $("#boss-mini");
  const show = active || bosses.find(b => b.status === "upcoming");
  if (!show) { mini.innerHTML = `<div class="muted">Aucun World Boss pour le moment.</div>`; return; }
  mini.innerHTML = `<div class="bname">${esc(show.name)}</div><div class="bsub">${esc(show.form)} · Niv. ${show.level}</div>
    ${show.status === "active"
      ? `<div class="hp"><div style="width:${Math.max(0, Math.min(100, show.hp_percent))}%"></div></div><div class="hp-row"><span>${show.hp_percent.toFixed(1)} % de vie</span><span>${show.players} dresseurs</span></div><div class="hp-row"><span>Fin dans</span><span data-end="${esc(show.ends_at)}">${countdown(show.ends_at)}</span></div>`
      : `<div class="hp-row"><span>Apparition dans</span><span data-end="${esc(show.ends_at)}">${countdown(show.ends_at)}</span></div>`}`;
}
setInterval(() => $$("[data-end]").forEach(el => el.textContent = countdown(el.dataset.end)), 1000);

document.addEventListener("click", e => {
  const u = e.target.closest("[data-url]"); if (u) return go(u.dataset.url);
  const n = e.target.closest("[data-nav]"); if (n) showPage(n.dataset.nav);
});

async function refreshRemote() {
  try {
    const res = await invoke("fetch_remote");
    state.remote = res.info; state.offline = res.offline;
  } catch (e) { state.remote = null; }
  renderRemote();
  await refreshInstall();
  refreshStatus();
}
async function refreshInstall() {
  if (!state.remote) return;
  state.install = await invoke("install_state", { minecraft: state.remote.minecraft, neoforge: state.remote.neoforge });
  state.running = state.install.running;
  renderPlay();
}
async function refreshStatus() {
  const addr = state.remote?.server?.address; if (!addr) return;
  const s = await invoke("server_status", { address: addr });
  $("#srv-dot").className = "status-dot " + (s.online ? "on" : "off");
  $("#srv-players").textContent = s.online ? `${s.players}/${s.max}` : "Hors ligne";
  $("#srv-ping").textContent = s.online ? s.ping_ms : "—";
}
setInterval(refreshStatus, 30000);
setInterval(refreshRemote, 120000);

// ---------------------------------------------------------------------------
// Paramètres
// ---------------------------------------------------------------------------
const PRESETS = {
  performance: { render_distance: 8, simulation_distance: 8, max_fps: 120, graphics_mode: 0, particles: 2, clouds: "false", biome_blend: 1, entity_scale: 0.5, mipmaps: 0, vsync: false },
  balanced: { render_distance: 12, simulation_distance: 10, max_fps: 144, graphics_mode: 1, particles: 1, clouds: "fast", biome_blend: 3, entity_scale: 1.0, mipmaps: 2, vsync: false },
  quality: { render_distance: 20, simulation_distance: 12, max_fps: 260, graphics_mode: 1, particles: 0, clouds: "true", biome_blend: 5, entity_scale: 1.5, mipmaps: 4, vsync: false },
};
const SLIDERS = {
  render_distance: v => v + " chunks", simulation_distance: v => v + " chunks",
  max_fps: v => v >= 260 ? "Illimité" : v + " FPS", biome_blend: v => v === 0 ? "Désactivé" : (v * 2 + 1) + "×" + (v * 2 + 1),
  entity_scale: v => Number(v).toFixed(1) + "×", mipmaps: v => v,
};
let saveTimer = null;
function scheduleSave() {
  $("#save-note").textContent = "Enregistrement…";
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try { await invoke("save_settings", { settings: state.settings }); $("#save-note").textContent = "Enregistré"; } catch (e) { $("#save-note").textContent = "Erreur : " + e; }
    renderPlay(); $("#meta-ram").textContent = (state.settings.ram_mb / 1024).toFixed(state.settings.ram_mb % 1024 ? 1 : 0) + " Go";
  }, 350);
}
function setSetting(k, v, fromPreset = false) {
  state.settings[k] = v;
  if (!fromPreset && k in PRESETS.balanced) { state.settings.preset = "custom"; }
  renderSettings(); scheduleSave();
}
function renderRam() {
  const s = state.settings, total = state.sys?.total_ram_mb || 8192;
  const max = Math.max(2048, Math.min(32768, Math.floor((total - 2048) / 512) * 512));
  const ram = $("#ram"); ram.max = max; ram.min = 2048; ram.value = Math.min(s.ram_mb, max);
  $("#ram-val").textContent = (s.ram_mb / 1024).toFixed(s.ram_mb % 1024 ? 1 : 0) + " Go";
  $("#ram-use").style.width = Math.min(100, s.ram_mb / total * 100) + "%";
  const rec = Math.min(max, total >= 16000 ? 8192 : total >= 12000 ? 6144 : 4096);
  $("#ram-hint").textContent = `Mémoire de ton PC : ${(total / 1024).toFixed(0)} Go. Recommandé pour ce modpack : ${(rec / 1024)} Go. Au-delà, le jeu peut ralentir.`;
}
function renderSettings() {
  const s = state.settings; if (!s) return;
  renderRam();
  for (const [k, f] of Object.entries(SLIDERS)) { const el = $("#" + k); el.value = s[k]; $("#" + k + "-val").textContent = f(s[k]); }
  for (const k of ["optimized_jvm", "vsync", "fullscreen", "auto_join", "close_on_launch"]) $("#" + k).checked = !!s[k];
  for (const k of ["java_path", "jvm_extra", "remote_url", "ms_client_id", "width", "height"]) { const el = $("#" + k); if (document.activeElement !== el) el.value = s[k]; }
  $$("#preset-seg button").forEach(b => b.classList.toggle("on", b.dataset.preset === s.preset));
  $$("[data-seg]").forEach(seg => { const k = seg.dataset.seg; $$("button", seg).forEach(b => b.classList.toggle("on", String(s[k]) === b.dataset.v)); });
}
function bindSettings() {
  $("#ram").oninput = e => setSetting("ram_mb", +e.target.value);
  for (const k of Object.keys(SLIDERS)) $("#" + k).oninput = e => setSetting(k, +e.target.value);
  for (const k of ["optimized_jvm", "vsync", "fullscreen", "auto_join", "close_on_launch"]) $("#" + k).onchange = e => setSetting(k, e.target.checked);
  for (const k of ["java_path", "jvm_extra", "remote_url", "ms_client_id"]) $("#" + k).oninput = e => { state.settings[k] = e.target.value.trim(); scheduleSave(); };
  for (const k of ["width", "height"]) $("#" + k).oninput = e => { const n = parseInt(e.target.value, 10); if (n > 0) { state.settings[k] = n; scheduleSave(); } };
  $$("#preset-seg button").forEach(b => b.onclick = () => {
    const p = b.dataset.preset;
    if (p === "custom") { state.settings.preset = "custom"; }
    else { Object.assign(state.settings, PRESETS[p], { preset: p }); }
    renderSettings(); scheduleSave();
  });
  $$("[data-seg] button").forEach(b => b.onclick = () => {
    const k = b.parentElement.dataset.seg;
    const raw = b.dataset.v; const v = (k === "clouds") ? raw : +raw;
    setSetting(k, v);
  });
  $("#open-data").onclick = () => invoke("open_data_folder");
  $("#repair-btn").onclick = async () => {
    if (!state.remote) return;
    showPage("home"); state.busy = true; renderPlay(); hideError();
    showProgress({ label: "Réparation de l'installation…", current: 0, total: 0 });
    try { await invoke("install_or_update", { info: state.remote }); await refreshInstall(); toast("Installation vérifiée."); } catch (e) { showError(e); }
    state.busy = false; hideProgress(); renderPlay();
  };
}

// ---------------------------------------------------------------------------
// Démarrage
// ---------------------------------------------------------------------------
(async function init() {
  bindSettings();
  state.settings = await invoke("get_settings");
  state.sys = await invoke("system_info");
  state.account = await invoke("get_account");
  $("#ver").textContent = "v" + state.sys.version;
  $("#data-dir").textContent = state.sys.data_dir;
  renderSettings(); renderAccount();
  await refreshRemote();
})();
