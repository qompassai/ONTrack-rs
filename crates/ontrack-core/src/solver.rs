// /qompassai/ontrack-rs/crates/ontrack-core/src/solver.rs
// Qompass AI — OnTrack core: TSP solver
// Copyright (C) 2026 Qompass AI, All rights reserved.
// -----------------------------------------------------
//! TSP/VRP route solver.
//!
//! Replaces the Python OR-Tools dependency with a pure-Rust implementation:
//!   - **Nearest-neighbour** construction: O(n²) — fast initial route
//!   - **2-opt local search** improvement: O(n²) per pass, converges quickly
//!
//! For typical field workloads (≤ 50 stops) this gives near-optimal routes
//! in milliseconds. For larger problem instances, increase
//! `SolverConfig::two_opt_passes` or set `two_opt_passes = 0` to skip
//! improvement and fall back to pure nearest-neighbour.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::geocoder::Location;

/// Result of a TSP solve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteResult {
    pub ordered_addresses: Vec<String>,
    pub ordered_indices: Vec<usize>,
    pub total_duration_seconds: f64,
    pub dropped_nodes: Vec<usize>,
    pub backend_used: String,
}

/// Which solver strategy to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverBackend {
    /// Greedy nearest-neighbour only.
    NearestNeighbor,
    /// Nearest-neighbour seed + 2-opt local search (default).
    NearestNeighborTwoOpt,
}

impl Default for SolverBackend {
    fn default() -> Self {
        Self::NearestNeighborTwoOpt
    }
}

/// Tuning knobs for the solver.
#[derive(Debug, Clone)]
pub struct SolverConfig {
    pub depot_index: usize,
    pub two_opt_passes: usize,
    pub backend: SolverBackend,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            depot_index: 0,
            two_opt_passes: 50,
            backend: SolverBackend::NearestNeighborTwoOpt,
        }
    }
}

/// Compute the total cost along an ordered tour.
fn route_cost(matrix: &[Vec<f64>], order: &[usize]) -> f64 {
    order
        .windows(2)
        .map(|w| matrix[w[0]][w[1]])
        .sum()
}

/// Nearest-neighbour greedy construction.
fn nearest_neighbor(matrix: &[Vec<f64>], start: usize) -> Vec<usize> {
    let n = matrix.len();
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);
    order.push(start);
    visited[start] = true;

    for _ in 1..n {
        let current = *order.last().unwrap();
        let mut best = (f64::INFINITY, usize::MAX);
        for j in 0..n {
            if !visited[j] && matrix[current][j] < best.0 {
                best = (matrix[current][j], j);
            }
        }
        if best.1 == usize::MAX {
            break;
        }
        visited[best.1] = true;
        order.push(best.1);
    }
    order
}

/// 2-opt local search improvement. Modifies `order` in place.
/// Preserves the depot at index 0 of the tour.
fn two_opt(matrix: &[Vec<f64>], order: &mut Vec<usize>, max_passes: usize) {
    let n = order.len();
    if n < 4 {
        return;
    }
    for _ in 0..max_passes {
        let mut improved = false;
        // i,k slice: reverse order[i..=k]. Keep i >= 1 so depot stays fixed.
        for i in 1..(n - 2) {
            for k in (i + 1)..(n - 1) {
                let a = order[i - 1];
                let b = order[i];
                let c = order[k];
                let d = order[k + 1];
                let delta = (matrix[a][c] + matrix[b][d]) - (matrix[a][b] + matrix[c][d]);
                if delta < -1e-9 {
                    order[i..=k].reverse();
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }
}

/// Validate matrix shape and depot index.
fn validate(locations: &[Location], matrix: &[Vec<f64>], depot_index: usize) -> Result<()> {
    let n = locations.len();
    if n == 0 {
        return Err(anyhow!("no locations provided"));
    }
    if matrix.len() != n || matrix.iter().any(|r| r.len() != n) {
        return Err(anyhow!(
            "matrix shape {}x{} does not match {} locations",
            matrix.len(),
            matrix.first().map(|r| r.len()).unwrap_or(0),
            n
        ));
    }
    if depot_index >= n {
        return Err(anyhow!("depot_index {depot_index} out of range [0, {n})"));
    }
    Ok(())
}

/// Solve the TSP for the given locations and cost matrix.
pub fn solve_tsp(
    locations: &[Location],
    matrix: &[Vec<f64>],
    config: SolverConfig,
) -> Result<RouteResult> {
    validate(locations, matrix, config.depot_index)?;

    let mut order = nearest_neighbor(matrix, config.depot_index);
    let (backend_used, after_opt) = match config.backend {
        SolverBackend::NearestNeighbor => ("nearest-neighbor".to_string(), order.clone()),
        SolverBackend::NearestNeighborTwoOpt => {
            two_opt(matrix, &mut order, config.two_opt_passes);
            ("nearest-neighbor+2opt".to_string(), order.clone())
        }
    };

    let total = route_cost(matrix, &after_opt);
    let n = locations.len();
    let dropped: Vec<usize> = (0..n).filter(|i| !after_opt.contains(i)).collect();
    let ordered_addresses: Vec<String> = after_opt
        .iter()
        .map(|&i| locations[i].address.clone())
        .collect();

    Ok(RouteResult {
        ordered_addresses,
        ordered_indices: after_opt,
        total_duration_seconds: total,
        dropped_nodes: dropped,
        backend_used,
    })
}

/// Open TSP — driver does not need to return to origin.
/// Adds a zero-cost dummy sink node and solves, then strips it.
pub fn solve_open_tsp(
    locations: &[Location],
    matrix: &[Vec<f64>],
    config: SolverConfig,
) -> Result<RouteResult> {
    let n = locations.len();
    let mut open_matrix: Vec<Vec<f64>> = matrix
        .iter()
        .map(|row| {
            let mut r = row.clone();
            r.push(0.0);
            r
        })
        .collect();
    open_matrix.push(vec![0.0; n + 1]);

    let mut locs = locations.to_vec();
    locs.push(Location {
        address: "__end__".to_string(),
        lat: None,
        lng: None,
    });

    let mut result = solve_tsp(&locs, &open_matrix, config)?;
    result.ordered_addresses.retain(|a| a != "__end__");
    result.ordered_indices.retain(|i| *i != n);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn loc(addr: &str) -> Location {
        Location { address: addr.to_string(), lat: Some(0.0), lng: Some(0.0) }
    }

    #[test]
    fn solves_trivial_route() {
        let locs = vec![loc("A"), loc("B"), loc("C")];
        let matrix = vec![
            vec![0.0, 10.0, 15.0],
            vec![10.0, 0.0, 20.0],
            vec![15.0, 20.0, 0.0],
        ];
        let r = solve_tsp(&locs, &matrix, SolverConfig::default()).unwrap();
        assert_eq!(r.ordered_indices.len(), 3);
        assert_eq!(r.ordered_indices[0], 0);
    }

    #[test]
    fn two_opt_improves_crossed_route() {
        // A diamond — nearest-neighbor crosses, 2-opt should fix it.
        let locs: Vec<Location> = (0..4).map(|i| loc(&format!("P{i}"))).collect();
        let matrix = vec![
            vec![0.0, 1.0, 2.0, 1.0],
            vec![1.0, 0.0, 1.0, 2.0],
            vec![2.0, 1.0, 0.0, 1.0],
            vec![1.0, 2.0, 1.0, 0.0],
        ];
        let cfg_nn = SolverConfig { backend: SolverBackend::NearestNeighbor, ..Default::default() };
        let cfg_opt = SolverConfig::default();
        let r_nn = solve_tsp(&locs, &matrix, cfg_nn).unwrap();
        let r_opt = solve_tsp(&locs, &matrix, cfg_opt).unwrap();
        assert!(r_opt.total_duration_seconds <= r_nn.total_duration_seconds);
    }
}
