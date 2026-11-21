use chrono::Utc;
use iris_core::VoiceIntentResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedVoiceAction {
    NavigateScreen(String),
    TriggerAgent { agent: String, prompt: String },
    Printer3DPowerOn,
    Printer3DCooldown,
    None,
}

pub struct VoiceIntentEngine;

impl VoiceIntentEngine {
    pub fn parse(text: &str) -> (VoiceIntentResult, ParsedVoiceAction) {
        let t = text.trim().to_lowercase();
        let now_ms = Utc::now().timestamp_millis();

        // 1. Actions d'agents spécifiques
        let is_pr_intent = t.contains("pull request")
            || t.contains("update pr")
            || t.contains("mise à jour des pr")
            || t.contains("mettre à jour les pr")
            || t.contains("mettre à jour les projets")
            || t.contains("rebase pr")
            || t.split_whitespace().any(|w| w == "pr" || w == "prs");

        if is_pr_intent {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "ACTION_AGENT".to_string(),
                spoken_reply: "Lancement de l'agent de mise à jour des Pull Requests sur jo3."
                    .to_string(),
                target_screen_id: Some("org-agents".to_string()),
                action_executed: Some("trigger_agent:antigravity-pr-updater".to_string()),
                timestamp_epoch_ms: now_ms,
            };
            let action = ParsedVoiceAction::TriggerAgent {
                agent: "antigravity-pr-updater".to_string(),
                prompt: "Clone jo3, helmscharts et dépôts actifs, inspecte et mets à jour les Pull Requests.".to_string(),
            };
            return (res, action);
        }

        if t.contains("diagnostic")
            || t.contains("diagnostique")
            || (t.contains("santé") && t.contains("cluster"))
            || t.contains("analyse le cluster")
        {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "ACTION_AGENT".to_string(),
                spoken_reply: "Mission de diagnostic du cluster transmise à Amaraki.".to_string(),
                target_screen_id: Some("org-agents".to_string()),
                action_executed: Some("trigger_agent:agent-k8s-diagnostician".to_string()),
                timestamp_epoch_ms: now_ms,
            };
            let action = ParsedVoiceAction::TriggerAgent {
                agent: "agent-k8s-diagnostician".to_string(),
                prompt: "Analyse les anomalies des pods, les restarts fréquents et l'état de santé du cluster jo3.".to_string(),
            };
            return (res, action);
        }

        if t.contains("remediat")
            || t.contains("remédiat")
            || t.contains("répare le cluster")
            || t.contains("dérive gitops")
        {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "ACTION_AGENT".to_string(),
                spoken_reply: "Remédiation GitOps et ArgoCD lancée.".to_string(),
                target_screen_id: Some("org-agents".to_string()),
                action_executed: Some("trigger_agent:opencode-cluster-remediator".to_string()),
                timestamp_epoch_ms: now_ms,
            };
            let action = ParsedVoiceAction::TriggerAgent {
                agent: "opencode-cluster-remediator".to_string(),
                prompt: "Détecte les dérives GitOps et applications ArgoCD dégradées puis remédie."
                    .to_string(),
            };
            return (res, action);
        }

        if t.contains("leclerc") || t.contains("panier") || t.contains("courses") {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "ACTION_AGENT".to_string(),
                spoken_reply: "Vérification du panier Leclerc Drive en cours.".to_string(),
                target_screen_id: Some("org-agents".to_string()),
                action_executed: Some("trigger_agent:opencode-leclerc".to_string()),
                timestamp_epoch_ms: now_ms,
            };
            let action = ParsedVoiceAction::TriggerAgent {
                agent: "opencode-leclerc".to_string(),
                prompt: "Consulte le panier actuel Leclerc Drive et synthétise les articles."
                    .to_string(),
            };
            return (res, action);
        }

        if t.contains("mail")
            || t.contains("email")
            || t.contains("urgence")
            || t.contains("courriel")
        {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "ACTION_AGENT".to_string(),
                spoken_reply: "Synthèse des emails prioritaires demandée à Amaraki.".to_string(),
                target_screen_id: Some("org-agents".to_string()),
                action_executed: Some("trigger_agent:opencode-mail".to_string()),
                timestamp_epoch_ms: now_ms,
            };
            let action = ParsedVoiceAction::TriggerAgent {
                agent: "opencode-mail".to_string(),
                prompt: "Vérifie les emails non lus reçus depuis hier et synthétise les urgences critiques.".to_string(),
            };
            return (res, action);
        }

        // 2. Actions 3D Printing directes
        if (t.contains("éteins") || t.contains("coupe") || t.contains("refroidis"))
            && (t.contains("imprimante") || t.contains("3d") || t.contains("ender"))
        {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "ACTION_3D".to_string(),
                spoken_reply: "Refroidissement et coupure programmée de l'imprimante 3D activés."
                    .to_string(),
                target_screen_id: Some("org-3d-print".to_string()),
                action_executed: Some("printer3d:cooldown_and_off".to_string()),
                timestamp_epoch_ms: now_ms,
            };
            return (res, ParsedVoiceAction::Printer3DCooldown);
        }

        if (t.contains("allume") || t.contains("démarre") || t.contains("active"))
            && (t.contains("imprimante") || t.contains("3d") || t.contains("ender"))
        {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "ACTION_3D".to_string(),
                spoken_reply: "Alimentation de l'imprimante 3D enclenchée.".to_string(),
                target_screen_id: Some("org-3d-print".to_string()),
                action_executed: Some("printer3d:power_on".to_string()),
                timestamp_epoch_ms: now_ms,
            };
            return (res, ParsedVoiceAction::Printer3DPowerOn);
        }

        // 3. Bascules de thèmes / Navigation
        if t.contains("3d")
            || t.contains("impression")
            || t.contains("ender")
            || t.contains("imprimante")
            || t.contains("klipper")
        {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "THEME_3D".to_string(),
                spoken_reply: "Voici l'état complet de l'impression 3D et des températures."
                    .to_string(),
                target_screen_id: Some("org-3d-print".to_string()),
                action_executed: None,
                timestamp_epoch_ms: now_ms,
            };
            return (
                res,
                ParsedVoiceAction::NavigateScreen("org-3d-print".to_string()),
            );
        }

        if t.contains("domotique")
            || t.contains("maison")
            || t.contains("salon")
            || t.contains("bureau")
            || t.contains("lumière")
            || t.contains("prise")
            || t.contains("énergie")
            || t.contains("energie")
            || t.contains("solaire")
        {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "THEME_DOMOTIQUE".to_string(),
                spoken_reply: "Affichage des zones de domotique et de l'énergie.".to_string(),
                target_screen_id: Some("org-home".to_string()),
                action_executed: None,
                timestamp_epoch_ms: now_ms,
            };
            return (
                res,
                ParsedVoiceAction::NavigateScreen("org-home".to_string()),
            );
        }

        if t.contains("cluster")
            || t.contains("kubernetes")
            || t.contains("k8s")
            || t.contains("kusanagi")
            || t.contains("infra")
            || t.contains("pod")
            || t.contains("node")
            || t.contains("proxmox")
            || t.contains("argocd")
        {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "THEME_CLUSTER".to_string(),
                spoken_reply: "Voici la synthèse du cluster Kubernetes jo3.".to_string(),
                target_screen_id: Some("org-life-infra".to_string()),
                action_executed: None,
                timestamp_epoch_ms: now_ms,
            };
            return (
                res,
                ParsedVoiceAction::NavigateScreen("org-life-infra".to_string()),
            );
        }

        if t.contains("agent")
            || t.contains("playbook")
            || t.contains("amaraki")
            || t.contains("antigravity")
            || t.contains("rapport")
            || t.contains("mission")
        {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "THEME_AGENTS".to_string(),
                spoken_reply: "Voici le centre de pilotage des agents et leurs rapports."
                    .to_string(),
                target_screen_id: Some("org-agents".to_string()),
                action_executed: None,
                timestamp_epoch_ms: now_ms,
            };
            return (
                res,
                ParsedVoiceAction::NavigateScreen("org-agents".to_string()),
            );
        }

        if t.contains("actualité") || t.contains("news") || t.contains("veille") {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "THEME_NEWS".to_string(),
                spoken_reply: "Affichage des actualités et du fil de veille.".to_string(),
                target_screen_id: Some("org-news".to_string()),
                action_executed: None,
                timestamp_epoch_ms: now_ms,
            };
            return (
                res,
                ParsedVoiceAction::NavigateScreen("org-news".to_string()),
            );
        }

        if t.contains("planning") || t.contains("agenda") || t.contains("calendrier") {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "THEME_AGENDA".to_string(),
                spoken_reply: "Voici votre planning de la semaine.".to_string(),
                target_screen_id: Some("org-agenda".to_string()),
                action_executed: None,
                timestamp_epoch_ms: now_ms,
            };
            return (
                res,
                ParsedVoiceAction::NavigateScreen("org-agenda".to_string()),
            );
        }

        if t.contains("note") || t.contains("tâche") || t.contains("todo") {
            let res = VoiceIntentResult {
                raw_text: text.to_string(),
                intent_type: "THEME_NOTES".to_string(),
                spoken_reply: "Affichage de vos notes et rappels.".to_string(),
                target_screen_id: Some("org-notes".to_string()),
                action_executed: None,
                timestamp_epoch_ms: now_ms,
            };
            return (
                res,
                ParsedVoiceAction::NavigateScreen("org-notes".to_string()),
            );
        }

        // Intention par défaut
        let res = VoiceIntentResult {
            raw_text: text.to_string(),
            intent_type: "CONVERSATION".to_string(),
            spoken_reply: format!("Commande comprise : {text}. Demandez-moi l'impression 3D, la domotique, le cluster ou un agent."),
            target_screen_id: None,
            action_executed: None,
            timestamp_epoch_ms: now_ms,
        };
        (res, ParsedVoiceAction::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_3d_print_intent() {
        let (res, act) = VoiceIntentEngine::parse("Montre moi l'impression 3d");
        assert_eq!(res.intent_type, "THEME_3D");
        assert_eq!(res.target_screen_id.as_deref(), Some("org-3d-print"));
        assert_eq!(
            act,
            ParsedVoiceAction::NavigateScreen("org-3d-print".to_string())
        );
    }

    #[test]
    fn parses_agent_pr_trigger() {
        let (res, act) = VoiceIntentEngine::parse("Mets à jour les PR de jo3");
        assert_eq!(res.intent_type, "ACTION_AGENT");
        match act {
            ParsedVoiceAction::TriggerAgent { agent, .. } => {
                assert_eq!(agent, "antigravity-pr-updater")
            }
            _ => panic!("Expected TriggerAgent"),
        }
    }

    #[test]
    fn parses_cluster_intent() {
        let (res, _) = VoiceIntentEngine::parse("Focus sur le cluster Kubernetes");
        assert_eq!(res.intent_type, "THEME_CLUSTER");
        assert_eq!(res.target_screen_id.as_deref(), Some("org-life-infra"));
    }
}
