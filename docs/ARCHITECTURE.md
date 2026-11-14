# Revue d'Architecture — Iris

## 1. Principes directeurs (inspirés de Pylos & Kusanagi)

Iris applique une **Clean Architecture multi-crates** (modèle *Pylos*) combinée à un **serveur temps réel Axum + MQTT `rumqttc` + PWA embarquée** (modèle *Kusanagi*) :

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Tablette Android (PWA Kiosque)                  │
│   index.html / app.js  <─── WebSocket (/ws) ───>  REST (/api/*)         │
│   (iframes OpenObserve / Grafana + Hub Org + Notes + Horloge)           │
└──────────────────────────────────▲──────────────────────────────────────┘
                                   │
┌──────────────────────────────────▼──────────────────────────────────────┐
│                         iris-server (Axum 0.8 + rust-embed)             │
│  • /ws (broadcast temps réel)                                           │
│  • /api/webhooks/openobserve & /api/webhooks/grafana                    │
│  • /api/health & /metrics (Prometheus)                                  │
└──────────────┬───────────────────────────────────────────┬──────────────┘
               │                                           │
┌──────────────▼──────────────┐             ┌──────────────▼──────────────┐
│     iris-application        │             │    iris-infrastructure      │
│  • IrisController (deep mod)│◄───Ports───►│  • RumqttcAdapter (MQTT)    │
│  • Rotation loop            │             │  • JsonFileRepository       │
│  • Incident override/restore│             └──────────────┬──────────────┘
└──────────────┬──────────────┘                            │
               │                                           ▼
┌──────────────▼──────────────┐                   Broker MQTT (EMQX)
│         iris-core           │              iris/<deviceId>/command/#
│  • Modèles purs & Erreurs   │              iris/<deviceId>/state
│  • MqttCommandParser        │              iris/<deviceId>/availability
│  • DashboardUrlFormatter    │
└─────────────────────────────┘
```

## 2. Séparation des responsabilités & Seams

1. **`iris-core`** : aucune dépendance I/O, aucun runtime async. Contient les types métiers (`DisplayScreen`, `ActiveAlert`, `OrgNote`, `IrisSettings`), le parseur de commandes MQTT (`MqttCommandParser`) et le formateur d'URLs kiosque (`DashboardUrlFormatter`).
2. **`iris-application`** : orchestre les cas d'usage via `IrisController` (module profond encapsulant le `RwLock`, le canal `broadcast::Sender<IrisSnapshot>`, la boucle de rotation et la machine à états d'incident). Communique avec l'extérieur uniquement via les traits `StateRepository` et `MqttPublisher`.
3. **`iris-infrastructure`** : implémente `StateRepository` (`JsonFileRepository` avec écriture atomique `.tmp` + `rename`) et `MqttPublisher` (`RumqttcAdapter` avec LWT `offline`, reconnexion automatique et publication retained sur `.../state`).
4. **`iris-server`** : expose l'API HTTP, les webhooks OpenObserve/Grafana, le flux WebSocket `/ws`, les métriques Prometheus `/metrics`, et embarque la PWA tablette via `rust-embed` (binaire unique sans dépendance Node.js au runtime).

## 3. Flux d'incident (Alert Override)

1. Une alerte arrive soit via **MQTT** (`iris/tablet-1/command/alert`), soit via **Webhook HTTP** (`POST /api/webhooks/openobserve` ou `/api/webhooks/grafana`).
2. `IrisController::trigger_alert` sauvegarde l'écran courant (`previous_screen_id_before_alert`), verrouille la rotation automatique, et bascule immédiatement la tablette vers :
   - `screenId` si fourni,
   - sinon une vue transitoire sur `dashboardUrl` si fournie,
   - sinon l'écran configuré avec `isAlertTarget = true`.
3. Le snapshot est diffusé instantanément à la tablette via WebSocket `/ws` et publié sur MQTT `.../state`.
4. À la résolution (`clear_alert` ou `status: resolved`), l'écran précédent est restauré et la rotation reprend.

## 4. Qualité & Pipelines CI/CD

- **Strict Clippy** : `-D warnings -D clippy::unwrap_used -D clippy::expect_used` sur toutes les bibliothèques et binaires.
- **PR (`pr-tests.yml`)** : `cargo fmt --check`, Clippy strict, `cargo test --workspace`.
- **Main (`ci.yml`)** : validation Rust + build Docker multi-arch (`linux/amd64`, `linux/arm64`) vers GHCR.
- **Promotion (`promote.yml`)** : création de manifeste multi-arch sur tag `v*.*.*`.
- **Sécurité (`secret-guard.yml`)** : scan Gitleaks sur chaque push/PR.
