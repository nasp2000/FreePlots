// SPDX-License-Identifier: MIT
//! A small derivative-free optimiser: the Nelder–Mead simplex.
//!
//! It is used by the non-linear curve fits. The engine has no dependency on an
//! optimisation library, and the simplex is enough for the two-parameter
//! surfaces of the curve-fit models.

use crate::error::{Error, Result};

/// Result of an optimisation.
#[derive(Debug, Clone, PartialEq)]
pub struct Minimum {
    /// The arguments of the minimum.
    pub arguments: Vec<f64>,
    /// The value of the function at the minimum.
    pub value: f64,
    /// Iterations used.
    pub iterations: usize,
}

/// Minimises `function` with the Nelder–Mead simplex.
///
/// `start` is the initial point and `steps` the size of the simplex along each
/// coordinate. The algorithm stops when the simplex is smaller than
/// `tolerance` or after `max_iterations` steps.
pub fn nelder_mead<F: FnMut(&[f64]) -> f64>(
    start: &[f64],
    steps: &[f64],
    max_iterations: usize,
    tolerance: f64,
    mut function: F,
) -> Result<Minimum> {
    let dimension = start.len();
    if dimension == 0 || steps.len() != dimension {
        return Err(Error::InvalidParameter {
            name: "optimiser dimension",
            value: dimension as f64,
        });
    }
    if max_iterations == 0 {
        return Err(Error::InvalidParameter {
            name: "max iterations",
            value: 0.0,
        });
    }

    // The simplex: the start point plus one point per coordinate.
    let mut simplex: Vec<Vec<f64>> = Vec::with_capacity(dimension + 1);
    simplex.push(start.to_vec());
    for index in 0..dimension {
        let mut point = start.to_vec();
        point[index] += steps[index];
        simplex.push(point);
    }
    let mut values: Vec<f64> = simplex.iter().map(|point| function(point)).collect();

    let mut iterations = 0;
    while iterations < max_iterations {
        iterations += 1;
        // Order the vertices by the function value: index 0 is the best and
        // index `dimension` the worst.
        let mut order: Vec<usize> = (0..=dimension).collect();
        order.sort_by(|&a, &b| {
            values[a]
                .partial_cmp(&values[b])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let sorted: Vec<Vec<f64>> = order.iter().map(|&i| simplex[i].clone()).collect();
        let sorted_values: Vec<f64> = order.iter().map(|&i| values[i]).collect();
        simplex = sorted;
        values = sorted_values;

        // The centroid of the best `dimension` vertices.
        let centroid: Vec<f64> = (0..dimension)
            .map(|j| {
                simplex
                    .iter()
                    .take(dimension)
                    .map(|point| point[j])
                    .sum::<f64>()
                    / dimension as f64
            })
            .collect();

        // Convergence: the simplex is small enough.
        let spread = simplex
            .iter()
            .map(|point| {
                point
                    .iter()
                    .zip(&centroid)
                    .map(|(value, mean)| (value - mean).abs())
                    .fold(0.0, f64::max)
            })
            .fold(0.0, f64::max);
        if spread < tolerance {
            break;
        }

        // Reflection through the centroid.
        let reflect: Vec<f64> = (0..dimension)
            .map(|j| centroid[j] + (centroid[j] - simplex[dimension][j]))
            .collect();
        let reflected = function(&reflect);

        if reflected < values[0] {
            // Expansion.
            let expand: Vec<f64> = (0..dimension)
                .map(|j| centroid[j] + 2.0 * (reflect[j] - centroid[j]))
                .collect();
            let expanded = function(&expand);
            if expanded < reflected {
                simplex[dimension] = expand;
                values[dimension] = expanded;
            } else {
                simplex[dimension] = reflect;
                values[dimension] = reflected;
            }
        } else if reflected < values[dimension - 1] {
            // The reflection is good enough.
            simplex[dimension] = reflect;
            values[dimension] = reflected;
        } else {
            // Contraction towards the centroid.
            let contract: Vec<f64> = (0..dimension)
                .map(|j| centroid[j] + 0.5 * (simplex[dimension][j] - centroid[j]))
                .collect();
            let contracted = function(&contract);
            if contracted < values[dimension] {
                simplex[dimension] = contract;
                values[dimension] = contracted;
            } else {
                // Shrink every vertex towards the best one.
                let best = simplex[0].clone();
                for index in 1..=dimension {
                    let shrunk: Vec<f64> = (0..dimension)
                        .map(|j| best[j] + 0.5 * (simplex[index][j] - best[j]))
                        .collect();
                    values[index] = function(&shrunk);
                    simplex[index] = shrunk;
                }
            }
        }
    }

    let best = (0..=dimension)
        .min_by(|&a, &b| {
            values[a]
                .partial_cmp(&values[b])
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0);
    Ok(Minimum {
        arguments: simplex[best].clone(),
        value: values[best],
        iterations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_simplex_finds_the_minimum_of_a_paraboloid() {
        let result = nelder_mead(&[5.0, -3.0], &[1.0, 1.0], 500, 1e-10, |point| {
            (point[0] - 1.0).powi(2) + (point[1] + 2.0).powi(2)
        })
        .expect("minimum");
        assert!((result.arguments[0] - 1.0).abs() < 1e-4, "{result:?}");
        assert!((result.arguments[1] + 2.0).abs() < 1e-4, "{result:?}");
        assert!(result.value < 1e-8, "{}", result.value);
    }

    #[test]
    fn the_simplex_handles_a_coupled_surface() {
        // The minimum of the coupled surface solves 3x − y = 2 and −x + 3y = 4,
        // which gives (1.25, 1.75).
        let result = nelder_mead(&[-1.0, 4.0], &[0.5, 0.5], 2000, 1e-12, |point| {
            (point[0] - 1.0).powi(2)
                + (point[1] - 2.0).powi(2)
                + 0.5 * (point[0] - point[1]).powi(2)
        })
        .expect("minimum");
        assert!((result.arguments[0] - 1.25).abs() < 1e-4, "{result:?}");
        assert!((result.arguments[1] - 1.75).abs() < 1e-4, "{result:?}");
    }

    #[test]
    fn invalid_arguments_are_rejected() {
        assert!(matches!(
            nelder_mead(&[], &[], 10, 1e-6, |_| 0.0),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            nelder_mead(&[1.0], &[1.0, 2.0], 10, 1e-6, |_| 0.0),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            nelder_mead(&[1.0], &[1.0], 0, 1e-6, |_| 0.0),
            Err(Error::InvalidParameter { .. })
        ));
    }
}
