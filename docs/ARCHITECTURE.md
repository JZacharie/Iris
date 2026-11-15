# Revue d'Architecture — Iris

## 1. Principes directeurs (inspirés de Pylos & Kusanagi)

Iris applique une **Clean Architecture multi-crates** (modèle *Pylos*) combinée à un **serveur temps réel Axum + MQTT `rumqttc` + PWA embarquée** (modèle *Kusanagi*) :

```
┌────────────────────────────────────────────────────────────────────────────────┐
│                      Tablette Android (PWA Kiosque + APK WebView)              │
│   index.html / app.js  <─── WebSocket (/ws) ───>  REST (/api/*)                │
│   • Hub Org, Maison & Énergie (HA), Vie & Infra (Kusanagi), Actualités         │
│   • Planning & ÉcoleDirecte, Stream Deck & PC, Notes, Horloge, Iframes         │
└───────────────────────────────────────▲────────────────────────────────────────┘
                                        │
┌───────────────────────────────────────▼────────────────────────────────────────┐
│                       iris-server (Axum 0.8 + rust-embed)                      │
│  • /ws (broadcast temps réel unique par lot) & /ws/pc (agent PC)               │
│  • org_sync (boucle rapide 60s HA+Kusanagi / boucle lente 600s parallèle)      │
│  • /api/webhooks/openobserve & /api/webhooks/grafana                           │
│  • /api/health & /metrics (Prometheus)                                         │
└──────────────────┬──────────────────────────────────────────────┬──────────────┘
                   │                                              │
┌──────────────────▼──────────────────┐        ┌──────────────────▼──────────────┐
│          iris-application           │        │       iris-infrastructure       │
│  • IrisController (deep module)     │◄─Seam─►│  • RumqttcAdapter (MQTT)        │
│  • apply_fast_telemetry (batch)     │        │  • JsonFileRepository           │
│  • apply_slow_feeds (batch)         │        │  • HomeAssistantClient (pooled) │
│  • Machine sémiotique Barthes       │        │  • KusanagiClient (5 endpoints) │
└──────────────────┬──────────────────┘        │  • EcoleDirecteClient (GTK v3)  │
                   │                           │  • OpenMeteo / Ical / K8s / GH  │
┌──────────────────▼──────────────────┐        └─────────────────────────────────┘
│              iris-core              │
│  • Modèles purs (PartialEq, Eq)     │
│  • MqttCommandParser                │
│  • DashboardUrlFormatter            │
└─────────────────────────────────────┘
```

## 2. Séparation des responsabilités, Profondeur (Depth) & Seams

1. **`iris-core`** : aucune dépendance I/O, aucun runtime async. Contient les types métiers purs (`DisplayScreen`, `ActiveAlert`, `HomeAssistantData`, `KusanagiInfraData`, `EcoleDirecteStatus`, `BarthesState`), le parseur de commandes MQTT (`MqttCommandParser`) et le formateur d'URLs kiosque (`DashboardUrlFormatter`).
2. **`iris-application`** : orchestre les cas d'usage via `IrisController` (*deep module* encapsulant le `RwLock`, le canal `broadcast::Sender<IrisSnapshot>`, la boucle de rotation, la machine sémiotique de Roland Barthes et la machine à états d'incident).
   - **Consolidation par lot (*Batch Locality*)** : `apply_fast_telemetry` (Home Assistant + Kusanagi toutes les 60 s) et `apply_slow_feeds` (Météo, iCal, K8s, GitHub PRs, Alexa, ÉcoleDirecte toutes les 600 s) acquièrent le verrou d'écriture une seule fois et émettent **un unique broadcast WebSocket** consolidé vers la tablette.
   - **Machine sémiotique Barthes enrichie** : `compute_barthes_state` intègre désormais les signaux faibles de Kusanagi (`alerts.critical_count > 0`, `argocd.degraded > 0`) dans l'état `Connotation`.
3. **`iris-infrastructure`** :
   - Implémente `StateRepository` (`JsonFileRepository` avec écriture atomique `.tmp` + `rename` et auto-insertion des nouveaux écrans `org-home` et `org-news`).
   - Implémente `MqttPublisher` (`RumqttcAdapter` avec LWT `offline`, reconnexion automatique et publication retained sur `.../state`).
   - Réutilise les pools HTTP/TLS (`reqwest::Client`) dans `HomeAssistantClient` et `KusanagiClient`, et exécute les 5 requêtes Kusanagi (`/api/k8s/cluster`, `/api/proxmox/nodes`, `/api/argocd/status`, `/api/alerts`, `/api/news`) en parallèle via `tokio::join!`.
4. **`iris-server`** : expose l'API HTTP, les webhooks OpenObserve/Grafana, le flux WebSocket `/ws`, les métriques Prometheus `/metrics`, et embarque la PWA tablette via `rust-embed`.

## 3. Qualité & Pipelines CI/CD

- **Strict Clippy** : `-D warnings -D clippy::unwrap_used -D clippy::expect_used` sur toutes les bibliothèques et binaires.
- **PR (`pr-tests.yml`)** : `cargo fmt --check`, Clippy strict, `cargo test --workspace`.
- **Main (`ci.yml`)** : validation Rust + build Docker multi-arch (`linux/amd64`, `linux/arm64`) vers GHCR (`ghcr.io/jzacharie/iris:latest`).
- **Promotion (`promote.yml`)** : création de manifeste multi-arch sur tag `v*.*.*`.
