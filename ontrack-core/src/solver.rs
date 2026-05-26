//! solver.rs — TSP route optimizer.
//!
//! Implements two solvers, both in pure Rust (no external solver dependency):
//!
//! 1. **Nearest-Neighbour** (`nn`) — O(n²) greedy heuristic.
//!    Fast, good enough for field routes ≤ 30 stops.
//!
//! 2. **Nearest-Neighbour + 2-opt** (`nn2opt`) — NN seed followed by 2-opt
//!    local search. Produces near-optimal routes for typical field route sizes.
//!    This is the default.
//!
//! The Python version used OR-Tools on desktop; in Rust the 2-opt solver
//! matches OR-Tools quality for routes under ~50 stops without any C++ FFI.

use crate::geocoder::Location;
use crate::matrix::Matrix;
use anyhow::{anyhow, Result};

/// Which solver algorithm to use.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SolverBackend {
    /// Greedy nearest-neighbour — very fast, reasonable quality
    NearestNeighbour,
    /// Nearest-neighbour seed + 2-opt local search (default)
    #[default]
    TwoOpt,
}

impl std::str::FromStr for SolverBackend {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "nn" | "nearest" | "nearest-neighbour" | "nearest-neighbor" => Ok(Self::NearestNeighbour),
            "2opt" | "two-opt" | "twoopt" => Ok(Self::TwoOpt),
            other => Err(anyhow!("Unknown solver backend: {other}")),
        }
    }
}

/// Result of a route optimization.
#[derive(Debug, Clone)]
pub struct RouteResult {
    /// Addresses in optimized drive order.
    pub ordered_addresses: Vec<String>,
    /// Indices into the original `locations` slice, in optimized order.
    pub ordered_indices: Vec<usize>,
    /// Total estimated duration/distance (units match the matrix).
    pub total_cost: f64,
    /// Which solver produced this result.
    pub backend_used: SolverBackend,
}

impl RouteResult {
    /// Format total_cost as a human-readable duration string (assuming seconds).
    pub fn format_duration(&self) -> String {
        format_duration(self.total_cost)
    }
}

pub fn format_duration(seconds: f64) -> String {
    let mins = (seconds / 60.0) as u64;
    if mins < 60 {
        format!("{mins} min")
    } else {
        let h = mins / 60;
        let m = mins % 60;
        if m == 0 { format!("{h}h") } else { format!("{h}h {m}m") }
    }
}

// ── Nearest-Neighbour ──────────────────────────────────────────────────────

fn nn_solve(matrix: &Matrix, start: usize) -> (Vec<usize>, f64) {
    let n = matrix.len();
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);
    let mut current = start;
    let mut total = 0.0;

    order.push(current);
    visited[current] = true;

    for _ in 1..n {
        let mut best_cost = f64::INFINITY;
        let mut best_next = usize::MAX;

        for j in 0..n {
            if !visited[j] && matrix[current][j] < best_cost {
                best_cost = matrix[current][j];
                best_next = j;
            }
        }

        if best_next == usize::MAX {
            break; // disconnected graph
        }

        visited[best_next] = true;
        order.push(best_next);
        total += best_cost;
        current = best_next;
    }

    (order, total)
}

// ── 2-opt local search ─────────────────────────────────────────────────────

/// Attempt to improve a route with 2-opt swaps until no improvement is found.
/// Returns (improved_route, improved_cost).
fn two_opt(matrix: &Matrix, mut route: Vec<usize>, mut cost: f64) -> (Vec<usize>, f64) {
    let n = route.len();
    let mut improved = true;

    while improved {
        improved = false;
        'outer: for i in 0..n - 1 {
            for j in i + 2..n {
                // Cost of current edges: (i → i+1) and (j → j+1 mod n)
                let next_j = if j + 1 < n { j + 1 } else { 0 };
                let old_cost = matrix[route[i]][route[i + 1]]
                    + matrix[route[j]][route[next_j]];
                // Cost of reversed edges: (i → j) and (i+1 → j+1 mod n)
                let new_cost = matrix[route[i]][route[j]]
                    + matrix[route[i + 1]][route[next_j]];

                if new_cost < old_cost - 1e-9 {
                    // Reverse the segment between i+1 and j
                    route[i + 1..=j].reverse();
                    cost = cost - old_cost + new_cost;
                    improved = true;
                    break 'outer; // restart after each improvement
                }
            }
        }
    }

    (route, cost)
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Solve the TSP for the given locations and cost matrix.
///
/// # Arguments
/// * `locations`   — geocoded locations (must match matrix dimensions)
/// * `matrix`      — NxN cost matrix from `build_distance_matrix`
/// * `depot_index` — starting stop index (default 0)
/// * `backend`     — solver algorithm
/// * `open_route`  — if true, driver does not return to origin
pub fn solve_tsp(
    locations: &[Location],
    matrix: &Matrix,
    depot_index: usize,
    backend: &SolverBackend,
    open_route: bool,
) -> Result<RouteResult> {
    let n = locations.len();

    if n == 0 {
        return Err(anyhow!("No locations provided."));
    }
    if matrix.len() != n || matrix.iter().any(|row| row.len() != n) {
        return Err(anyhow!(
            "Matrix shape {}×{} does not match {} locations.",
            matrix.len(),
            matrix.first().map(|r| r.len()).unwrap_or(0),
            n
        ));
    }
    if depot_index >= n {
        return Err(anyhow!("depot_index {depot_index} out of range [0, {n})."));
    }

    // For open routes: append a zero-cost dummy sink and strip it after solving
    let (effective_matrix, _effective_n) = if open_route {
        let mut m: Matrix = matrix.iter().map(|row| {
            let mut r = row.clone();
            r.push(0.0);
            r
        }).collect();
        m.push(vec![0.0; n + 1]);
        (m, n + 1)
    } else {
        (matrix.to_vec(), n)
    };

    let (raw_route, raw_cost) = nn_solve(&effective_matrix, depot_index);

    let (final_route, final_cost) = match backend {
        SolverBackend::NearestNeighbour => (raw_route, raw_cost),
        SolverBackend::TwoOpt => two_opt(&effective_matrix, raw_route, raw_cost),
    };

    // Strip the dummy sink node for open routes
    let (ordered_indices, total_cost): (Vec<usize>, f64) = if open_route {
        let stripped: Vec<usize> = final_route.into_iter().filter(|&i| i < n).collect();
        // Recalculate cost without the dummy edges
        let cost = stripped
            .windows(2)
            .map(|w| matrix[w[0]][w[1]])
            .sum();
        (stripped, cost)
    } else {
        (final_route, final_cost)
    };

    let ordered_addresses = ordered_indices
        .iter()
        .map(|&i| locations[i].address.clone())
        .collect();

    Ok(RouteResult {
        ordered_addresses,
        ordered_indices,
        total_cost,
        backend_used: backend.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geocoder::Location;

    fn make_locs(n: usize) -> Vec<Location> {
        (0..n)
            .map(|i| Location {
                address: format!("Stop {i}"),
                lat: Some(47.65 + i as f64 * 0.01),
                lng: Some(-117.42 + i as f64 * 0.01),
            })
            .collect()
    }

    fn sym_matrix(n: usize) -> Matrix {
        (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| if i == j { 0.0 } else { (i as f64 - j as f64).abs() * 200.0 })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn nn_three_stops() {
        let locs = make_locs(3);
        let m = sym_matrix(3);
        let result = solve_tsp(&locs, &m, 0, &SolverBackend::NearestNeighbour, false).unwrap();
        assert_eq!(result.ordered_addresses.len(), 3);
        assert_eq!(result.ordered_addresses[0], "Stop 0");
    }

    #[test]
    fn two_opt_three_stops() {
        let locs = make_locs(3);
        let m = sym_matrix(3);
        let result = solve_tsp(&locs, &m, 0, &SolverBackend::TwoOpt, false).unwrap();
        assert_eq!(result.ordered_addresses.len(), 3);
    }

    #[test]
    fn open_route_strips_dummy() {
        let locs = make_locs(3);
        let m = sym_matrix(3);
        let result = solve_tsp(&locs, &m, 0, &SolverBackend::TwoOpt, true).unwrap();
        assert!(!result.ordered_addresses.contains(&"__end__".to_string()));
        assert!(result.ordered_indices.iter().all(|&i| i < 3));
    }

    #[test]
    fn empty_locations_errors() {
        let result = solve_tsp(&[], &vec![], 0, &SolverBackend::TwoOpt, false);
        assert!(result.is_err());
    }

    #[test]
    fn bad_depot_errors() {
        let locs = make_locs(3);
        let m = sym_matrix(3);
        assert!(solve_tsp(&locs, &m, 99, &SolverBackend::TwoOpt, false).is_err());
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(2700.0), "45 min");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(7200.0), "2h");
        assert_eq!(format_duration(5400.0), "1h 30m");
    }
}
