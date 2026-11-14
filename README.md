# Iris — Observabilité & Organisation en Rust

**Iris** est un serveur Rust (Clean Architecture multi-crates inspirée de **Pylos** et **Kusanagi**) avec interface **PWA Kiosque** embarquée pour tablette Android :

- **Dashboards OpenObserve & Grafana** en mode kiosque (`?hide_header=true`, `?kiosk=tv&refresh=...`).
- **Bascule automatique sur incident** via **MQTT** (`iris/<deviceId>/command/alert`) ou **Webhooks HTTP** (`/api/webhooks/openobserve`, `/api/webhooks/grafana`).
- **Pilotage complet par MQTT** (`rumqttc`, LWT `availability`, état retained `state`).
- **Écrans pratiques d'organisation** : Hub Organisation, Notes & Checklist synchronisées par MQTT, Horloge Focus.

---

## Structure du Workspace Rust

- `crates/iris-core` : Modèles purs, `MqttCommandParser`, `DashboardUrlFormatter`.
- `crates/iris-application` : `IrisController` (rotation, override incident, notes, broadcast WebSocket).
- `crates/iris-infrastructure` : `RumqttcAdapter` (MQTT), `JsonFileRepository` (persistance atomique).
- `crates/iris-server` : Serveur `axum` (REST, WebSocket `/ws`, Webhooks, `/metrics`, PWA `rust-embed`).

Voir [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) pour la revue d'architecture détaillée.

---

## Démarrage rapide & CI locale

```powershell
# Vérification complète (fmt + clippy strict + tests + build)
.\local-ci.ps1

# Lancer le serveur en local
cargo run --bin iris-server
# Puis ouvrir http://localhost:8080 sur la tablette Android (Chrome -> Ajouter à l'écran d'accueil)
```

---

## Protocole MQTT (`iris/<deviceId>/command/#`)

| Topic | Payload exemple | Effet |
|---|---|---|
| `screen` | `next`, `prev`, `o2-overview` | Changer d'écran |
| `url` | `{"url":"https://grafana.zacharie.org/d/xyz","title":"CPU"}` | Ouvrir une URL temporaire |
| `alert` | `{"severity":"critical","title":"Pod CrashLoop","dashboardUrl":"https://o2-openobserve.p.zacharie.org/web/dashboards","source":"openobserve"}` | **Forcer** l'écran sur l'alerte |
| `alert` | `resolved` | Clôturer et restaurer l'écran précédent |
| `rotation` | `pause`, `resume`, `{"enabled":true,"intervalSeconds":45}` | Contrôler la rotation |
| `brightness` | `dim`, `full`, `35` | Régler la luminosité |
| `note` | `{"text":"Vérifier backup MinIO","category":"Infra","pinned":true}` | Ajouter une note d'organisation |
