use std::collections::BTreeMap;

use game_domain::SkillTree;
use game_net::SkillCatalogEntry;

use crate::{ProbeError, ProbeResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreePlan {
    pub tree: SkillTree,
    pub tiers: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchPlan {
    pub match_index: usize,
    pub players: Vec<TreePlan>,
}

pub fn build_match_plans(
    catalog: &[SkillCatalogEntry],
    players_per_match: usize,
    preferred_tree_order: Option<&[String]>,
) -> ProbeResult<(Vec<TreePlan>, Vec<MatchPlan>)> {
    if players_per_match == 0 {
        return Err(ProbeError::new(
            "players_per_match must be greater than zero",
        ));
    }
    if catalog.is_empty() {
        return Err(ProbeError::new("skill catalog is empty"));
    }

    let mut grouped: BTreeMap<SkillTree, Vec<u8>> = BTreeMap::new();
    for entry in catalog {
        grouped
            .entry(entry.tree.clone())
            .or_default()
            .push(entry.tier);
    }

    let mut trees = Vec::new();
    for (tree, mut tiers) in grouped {
        tiers.sort_unstable();
        tiers.dedup();
        let max_tier = *tiers.last().unwrap_or(&0);
        let expected: Vec<u8> = (1..=max_tier).collect();
        if tiers != expected {
            return Err(ProbeError::new(format!(
                "skill tree {tree} has a non-sequential authored tier set {tiers:?}"
            )));
        }
        trees.push(TreePlan { tree, tiers });
    }

    trees.sort_by(|left, right| {
        preferred_rank(preferred_tree_order, left.tree.as_str())
            .cmp(&preferred_rank(preferred_tree_order, right.tree.as_str()))
            .then_with(|| left.tree.as_str().cmp(right.tree.as_str()))
    });

    let mut plans = Vec::new();
    let mut start = 0usize;
    while start < trees.len() {
        let end = (start + players_per_match).min(trees.len());
        let mut players = trees[start..end].to_vec();
        let mut filler_index = 0usize;
        while players.len() < players_per_match {
            players.push(trees[filler_index % trees.len()].clone());
            filler_index += 1;
        }
        plans.push(MatchPlan {
            match_index: plans.len() + 1,
            players,
        });
        start = end;
    }

    Ok((trees, plans))
}

fn preferred_rank(preferred_tree_order: Option<&[String]>, tree_name: &str) -> usize {
    preferred_tree_order
        .and_then(|order| {
            order
                .iter()
                .position(|preferred| preferred.eq_ignore_ascii_case(tree_name))
        })
        .unwrap_or(usize::MAX)
}
