# ADR 0001 — Architecture Rust Clean Architecture & PWA Kiosque Tablette

- **Statut** : Accepté
- **Date** : 2026-11-14

## Contexte

Iris doit piloter un écran sur tablette Android pour afficher des dashboards Grafana / OpenObserve, basculer automatiquement sur un dashboard d'alerte lors d'un incident, être pilotable via MQTT, et fournir des écrans pratiques d'organisation. L'écosystème de référence du workspace (`Pylos`, `Kusanagi`) repose sur **Rust**.

## Décision

1. Implémenter le cœur applicatif, le moteur MQTT (`rumqttc`), la machine à états d'incident, les webhooks OpenObserve/Grafana et le serveur HTTP/WebSocket (`axum`) en **Rust**, découpé en 4 crates (`iris-core`, `iris-application`, `iris-infrastructure`, `iris-server`) selon le modèle de **Pylos**.
2. Embarquer l'interface tablette Android sous forme de **PWA Kiosque** (`manifest.json` fullscreen landscape + WebSocket `/ws`) via `rust-embed` dans le binaire `iris-server`, permettant un déploiement en binaire unique / conteneur Docker sur le cluster `jo3` (comme **Kusanagi**) tout en offrant une expérience plein écran native sur tablette Android.
