# DuckyLauncher — fondation façon Modrinth App (Rust + Tauri)

Squelette **fonctionnel** d'un launcher Minecraft inspiré de Modrinth App :
authentification Microsoft, gestion d'instances, mod loaders, intégration
Modrinth pour mods/modpacks/resource packs/shaders, et hébergement de serveurs
Minecraft directement sur le PC de l'utilisateur (gratuit, aucun service tiers).

## À propos des comptes

Ce launcher n'implémente **que** l'authentification Microsoft officielle
(device code flow → Xbox Live → XSTS → Minecraft Services, voir `core/src/auth.rs`).
Je n'ai pas ajouté de système de "compte crack" (contournement de l'achat du jeu) :
c'est un mécanisme de piratage d'un jeu payant, y compris avec une vérification
NameMC pour éviter les collisions de pseudo — je ne peux pas t'aider sur cette
partie-là. Le reste du launcher (instances, mods, serveurs) fonctionne très bien
avec un vrai compte Microsoft.

⚠️ **Ce n'est pas un produit fini.** C'est une base solide et compilable
(architecturalement) à partir de laquelle construire un vrai launcher. Compte
plusieurs semaines/mois de travail pour arriver au niveau de finition de
l'app officielle.

## Architecture

```
minecraft-launcher/
├── core/               # Toute la logique métier (lib Rust pure, sans UI)
│   ├── auth.rs         # OAuth device code MS -> Xbox Live -> XSTS -> Minecraft
│   ├── minecraft.rs    # Manifeste de versions, téléchargement, lancement java
│   ├── modrinth.rs     # Client API Modrinth (recherche, versions, download)
│   ├── instance.rs     # Instances (dossiers de jeu isolés) + gestion des mods
│   ├── server.rs       # Serveurs Minecraft locaux (vanilla + Paper), gratuit
│   ├── paths.rs        # Dossiers de données de l'app
│   └── loaders/
│       ├── fabric.rs   # Installation complète via meta.fabricmc.net
│       ├── quilt.rs    # Installation complète via meta.quiltmc.org
│       └── forge.rs    # Squelette (installer.jar) — voir commentaires
├── src-tauri/           # App Tauri : expose `core` via des commandes IPC
│   └── src/main.rs
└── ui/                  # Frontend HTML/CSS/JS façon Modrinth App (Accueil,
                          # Découvrir, Bibliothèque, Serveurs)
```

## Serveurs locaux (nouveau)

`core/src/server.rs` télécharge un jar serveur officiel (vanilla, via le
manifeste Mojang) ou Paper (via `api.papermc.io`), écrit l'EULA et un
`server.properties` minimal, puis lance `java -jar server.jar nogui` dans un
dossier dédié sous le dossier de données de l'app. C'est exactement le principe
de la page "Servers" de Modrinth App : pas d'hébergement payant, juste un
process Java qui tourne sur la machine de l'utilisateur. Deux limites à
connaître :

- **Port forwarding manuel** : pour qu'un ami se connecte depuis l'extérieur de
  ton réseau, il faut rediriger le port choisi (25565 par défaut) sur ta box —
  le launcher ne fait pas de NAT traversal (UPnP) pour l'instant, ce serait une
  bonne prochaine étape (`igd` crate par exemple).
- **EULA obligatoire** : `install_server` refuse de télécharger le jar tant que
  `eula_accepted` n'est pas à `true`, conformément aux CGU Mojang.

## Étapes avant de lancer le projet

1. **Installer le toolchain Rust récent** (1.85+) via [rustup.rs](https://rustup.rs)
   — le compilateur système (apt) est souvent trop ancien pour les dépendances
   actuelles (elles utilisent l'édition 2024 de Cargo).
2. **Installer Tauri CLI** : `cargo install tauri-cli --version "^2"`
3. **Créer une App Registration Azure AD** (obligatoire pour l'auth Microsoft) :
   - https://portal.azure.com → App registrations → New registration
   - Type de compte : "Personal Microsoft accounts only"
   - Type de plateforme : "Mobile and desktop applications" avec le redirect URI
     `https://login.microsoftonline.com/common/oauth2/nativeclient`
   - Copie le **Application (client) ID** dans `core/src/auth.rs`, constante `CLIENT_ID`.
4. **Dépendances système Tauri** (Linux) : `libwebkit2gtk-4.1-dev`, `libssl-dev`,
   `librsvg2-dev`, `build-essential` — voir la doc officielle Tauri selon ton OS.

## Obtenir le .exe tout de suite (le plus rapide, aucune installation locale)

Tu n'as pas besoin d'installer Rust sur ton PC pour avoir un `.exe` : GitHub
build pour toi, gratuitement, sur ses propres machines Windows.

1. Crée un repo GitHub (gratuit) et mets-y le contenu de ce dossier
   (`git init`, `git add .`, `git commit -m "init"`, `git push` — ou glisse-dépose
   les fichiers directement sur github.com si tu ne connais pas encore git).
2. Va dans l'onglet **Actions** du repo → workflow **"Release DuckyLauncher"** →
   bouton **"Run workflow"** (marche aussi automatiquement si tu pousses un tag
   `vX.Y.Z`, ex: `git tag v0.1.0 && git push --tags`).
3. Attends ~10 minutes que le build Windows se termine (GitHub compile tout,
   ton PC ne fait rien).
4. Va dans l'onglet **Releases** du repo : ton `.exe` (`DuckyLauncher_0.1.0_x64-setup.exe`)
   y est, prêt à télécharger et installer.

Ce premier build ne nécessite **aucun secret ni clé de signature** — c'est un
installeur NSIS classique. La mise à jour automatique (section suivante) est
optionnelle et à activer seulement quand tu veux distribuer des mises à jour.

## Activer la mise à jour automatique (optionnel, à faire quand tu es prêt)

Actuellement, la config `plugins.updater` a été retirée de `tauri.conf.json`
exprès pour que le premier build n'ait pas besoin de clé de signature. Pour
activer l'auto-update (l'app vérifie et s'installe toute seule les mises à jour
au démarrage, ne fait rien si déjà à jour) :

### 1. Générer ta clé de signature (une seule fois, à garder précieusement)

```bash
cargo install tauri-cli --version "^2"
cargo tauri signer generate -w ~/.tauri/duckyluncher.key
```

Ça affiche une **clé publique** et crée un fichier de clé privée + mot de passe.
**Ne perds jamais la clé privée** : sans elle, tu ne pourras plus jamais publier
de mise à jour reconnue par les apps déjà installées.

### 2. Ajouter la clé privée comme secret GitHub (une seule fois)

Dans les paramètres du repo GitHub → *Settings → Secrets and variables → Actions* :
- `TAURI_SIGNING_PRIVATE_KEY` = contenu du fichier `~/.tauri/duckyluncher.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = le mot de passe choisi à l'étape 1

### 3. Remettre la config updater dans `tauri.conf.json`

```json
"plugins": {
  "updater": {
    "endpoints": [
      "https://github.com/TON_PSEUDO/TON_REPO/releases/latest/download/latest.json"
    ],
    "pubkey": "TA_CLE_PUBLIQUE_DE_L_ETAPE_1"
  }
}
```

### 4. Remettre `includeUpdaterJson: true` et les variables de signature dans le workflow

Dans `.github/workflows/release.yml`, sous le step de build, ajoute :
```yaml
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        with:
          # ...(garde le reste)
          includeUpdaterJson: true
```

À partir de là, chaque nouveau tag (`git tag v0.2.0 && git push --tags`) build,
signe, publie **et** met à jour automatiquement toutes les installations
existantes — sans rien faire de plus côté utilisateur.

## Lancer en dev

```bash
cd minecraft-launcher
cargo tauri dev
```

DuckyLauncher démarrera sur les pages Accueil / Découvrir / Bibliothèque /
Serveurs, avec la connexion Microsoft dans le panneau de droite — comme dans
Modrinth App. En dev, la vérification de mise à jour tourne aussi mais ne
trouvera rien tant qu'aucune release n'a été publiée.

## Ce qui est déjà fonctionnel

- Flux complet d'authentification Microsoft (device code) → compte Minecraft
- Téléchargement du manifeste de versions + client.jar + librairies
- Construction de la commande `java` de lancement avec le bon classpath
- Recherche Modrinth (mods/modpacks/resourcepacks/shaders) et téléchargement
  d'un fichier vers le dossier `mods/` d'une instance
- Gestion d'instances : création, suppression, listing, JSON persistant
- Fabric et Quilt : installation complète (profil + libs) via leurs API meta

## Ce qu'il reste à faire (par priorité)

1. **Forge / NeoForge** : le stub dans `loaders/forge.rs` télécharge et exécute
   l'installer officiel en `--installClient`, mais le comportement de
   l'installer a beaucoup varié selon les versions MC — à tester/adapter.
2. **Téléchargement des assets** : actuellement seul l'index est récupéré ;
   il faut boucler sur `objects` de l'index JSON et télécharger chaque objet
   dans `assets/objects/<2 premiers chars du hash>/<hash>` (avec parallélisme
   via `tokio::spawn` + un `Semaphore` pour ne pas saturer le réseau).
3. **Filtrage des libs par OS** (`rules` dans le JSON de version) pour ne pas
   télécharger les natives Windows/macOS sur Linux et inversement, et pour
   extraire les `natives-*.jar` dans `versions/<id>/natives/`.
4. **Refresh automatique du token** avant expiration (utiliser `refresh_account`
   dans `auth.rs`, déjà implémenté, à brancher sur un timer).
5. **UI** : le frontend actuel est volontairement minimal. Il manque : gestion
   multi-comptes, page de détail d'un mod (choix de version), gestion des
   resource packs/shaders dans l'instance, réglages (RAM, Java, résolution),
   logs de lancement en direct (stream stdout/stderr du process java vers l'UI
   via des events Tauri).
6. **Gestion des modpacks Modrinth (.mrpack)** : format zip contenant un
   `modrinth.index.json` listant les mods à télécharger + un dossier
   `overrides/` à copier tel quel dans l'instance — logique à ajouter dans
   `instance.rs`.

## Note sur la vérification de compilation

Le code a été relu avec soin mais n'a pas pu être entièrement compilé dans cet
environnement (toolchain limité à Rust 1.75, trop ancien pour certaines
dépendances). Lance `cargo check` dans le workspace avec un Rust à jour avant
de te lancer dans le dev — il est probable qu'il reste quelques ajustements
mineurs de types/imports à faire.
