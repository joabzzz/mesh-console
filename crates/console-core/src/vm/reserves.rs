//! Reserves view-model: group `status.wakeable_nodes` by provider and sum
//! capacity, mirroring the real-data path of the web console's Reserves page
//! (`reserve-totals.ts` / `reserve-state.ts`; fixture data is intentionally
//! not replicated).

use crate::state::WakeableNode;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReserveTotals {
    pub node_count: usize,
    pub sleeping: usize,
    pub waking: usize,
    pub total_vram_gb: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderGroup {
    /// Provider name; nodes without one group under "unassigned".
    pub provider: String,
    pub nodes: Vec<WakeableNode>,
    pub totals: ReserveTotals,
}

pub fn totals(nodes: &[WakeableNode]) -> ReserveTotals {
    ReserveTotals {
        node_count: nodes.len(),
        sleeping: nodes.iter().filter(|n| n.state == "sleeping").count(),
        waking: nodes.iter().filter(|n| n.state == "waking").count(),
        total_vram_gb: nodes.iter().map(|n| n.vram_gb).sum(),
    }
}

/// Group by provider, providers sorted alphabetically with "unassigned" last;
/// nodes within a group keep server order.
pub fn group_by_provider(nodes: &[WakeableNode]) -> Vec<ProviderGroup> {
    let mut groups: Vec<ProviderGroup> = Vec::new();
    for node in nodes {
        let provider = node
            .provider
            .clone()
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "unassigned".to_string());
        match groups.iter_mut().find(|g| g.provider == provider) {
            Some(group) => group.nodes.push(node.clone()),
            None => groups.push(ProviderGroup {
                provider,
                nodes: vec![node.clone()],
                totals: ReserveTotals::default(),
            }),
        }
    }
    for group in &mut groups {
        group.totals = totals(&group.nodes);
    }
    groups.sort_by(|a, b| match (&*a.provider, &*b.provider) {
        ("unassigned", "unassigned") => std::cmp::Ordering::Equal,
        ("unassigned", _) => std::cmp::Ordering::Greater,
        (_, "unassigned") => std::cmp::Ordering::Less,
        (a, b) => a.cmp(b),
    });
    groups
}

/// "~2 min" style ETA label.
pub fn wake_eta_label(eta_secs: Option<u32>) -> Option<String> {
    let secs = eta_secs?;
    Some(if secs < 60 {
        format!("~{secs}s")
    } else {
        format!("~{} min", secs.div_ceil(60))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, provider: Option<&str>, state: &str, vram: f64) -> WakeableNode {
        WakeableNode {
            logical_id: id.to_string(),
            provider: provider.map(str::to_string),
            state: state.to_string(),
            vram_gb: vram,
            ..WakeableNode::default()
        }
    }

    #[test]
    fn groups_by_provider_with_unassigned_last() {
        let nodes = vec![
            node("a", None, "sleeping", 8.0),
            node("b", Some("fly.io"), "waking", 24.0),
            node("c", Some("wake-on-lan"), "sleeping", 48.0),
            node("d", Some("fly.io"), "sleeping", 24.0),
        ];
        let groups = group_by_provider(&nodes);
        let names: Vec<&str> = groups.iter().map(|g| g.provider.as_str()).collect();
        assert_eq!(names, vec!["fly.io", "wake-on-lan", "unassigned"]);
        assert_eq!(groups[0].nodes.len(), 2);
        assert_eq!(groups[0].totals.total_vram_gb, 48.0);
        assert_eq!(groups[0].totals.waking, 1);
    }

    #[test]
    fn totals_count_states_and_vram() {
        let nodes = vec![
            node("a", None, "sleeping", 8.0),
            node("b", None, "waking", 16.0),
        ];
        let t = totals(&nodes);
        assert_eq!(t.node_count, 2);
        assert_eq!(t.sleeping, 1);
        assert_eq!(t.waking, 1);
        assert_eq!(t.total_vram_gb, 24.0);
    }

    #[test]
    fn eta_labels() {
        assert_eq!(wake_eta_label(Some(45)).as_deref(), Some("~45s"));
        assert_eq!(wake_eta_label(Some(95)).as_deref(), Some("~2 min"));
        assert_eq!(wake_eta_label(None), None);
    }
}
