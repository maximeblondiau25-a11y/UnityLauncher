#!/usr/bin/env python3
"""Empaquette ton dossier de jeu (mods, config…) en zip et affiche le bloc "modpack" pour launcher.json.

Usage : python tools/make_modpack.py <dossier_instance> <version> [url_de_telechargement]
Le dossier doit contenir mods/, config/, etc. (le contenu part à la racine du zip).
Ne mets PAS saves/, logs/, options.txt dans le dossier : ils sont propres à chaque joueur.
"""
import hashlib, json, os, sys, zipfile

SKIP_DIRS = {"saves", "logs", "crash-reports", "screenshots", ".git"}
SKIP_FILES = {"options.txt", "servers.dat", "usercache.json"}

def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    root, version = sys.argv[1], sys.argv[2]
    url = sys.argv[3] if len(sys.argv) > 3 else "https://exemple.net/downloads/pokemon-unity-modpack-%s.zip" % version
    out = "pokemon-unity-modpack-%s.zip" % version
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for dp, dn, fn in os.walk(root):
            dn[:] = [d for d in dn if d not in SKIP_DIRS]
            for f in fn:
                if f in SKIP_FILES:
                    continue
                full = os.path.join(dp, f)
                z.write(full, os.path.relpath(full, root).replace(os.sep, "/"))
    h = hashlib.sha256()
    with open(out, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    block = {"version": version, "url": url, "sha256": h.hexdigest(), "size": os.path.getsize(out), "changelog": ""}
    print("Archive :", out)
    print('Colle ce bloc dans la clé "modpack" de launcher.json :')
    print(json.dumps(block, indent=2, ensure_ascii=False))

if __name__ == "__main__":
    main()
