use ark_ff::{Field, PrimeField};
use ark_serialize::CanonicalDeserialize;
use ark_std::log2;
use serde::{Serialize, Deserialize};
use std::fs::File;
use std::io::BufReader;
use ark_bls12_381::{G1Projective as G1, G2Projective as G2};
use hex;

#[derive(Serialize, Deserialize)]
struct EthereumCeremony {
    contributions: Vec<Contribution>,
}

#[derive(Serialize, Deserialize)]
struct Contribution {
    #[serde(rename = "numG1Powers")]
    num_g1_powers: usize,
    #[serde(rename = "numG2Powers")]
    num_g2_powers: usize,
    #[serde(rename = "powersOfTau")]
    powers_of_tau: PowersOfTau,
}

#[derive(Serialize, Deserialize)]
struct PowersOfTau {
    #[serde(rename = "G1Powers")]
    g1_powers: Vec<String>,
    #[serde(rename = "G2Powers")]
    g2_powers: Vec<String>,
}

/// Load Ethereum's trusted setup ceremony data and extract CRS powers
pub fn load_ethereum_ceremony(file_path: &str, degree: usize) -> Result<(Vec<G1>, Vec<G2>), Box<dyn std::error::Error>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    
    let ceremony: EthereumCeremony = serde_json::from_reader(reader)?;
    
    // Use the last contribution (most recent/final)
    let final_contribution = ceremony.contributions.last()
        .ok_or("No contributions found in ceremony file")?;
    
    // Extract G1 powers (we need degree + 1 powers)
    let mut crs_g1 = Vec::new();
    for i in 0..=degree {
        if i >= final_contribution.powers_of_tau.g1_powers.len() {
            return Err(format!("Ceremony only has {} G1 powers, but {} are needed", 
                final_contribution.powers_of_tau.g1_powers.len(), degree + 1).into());
        }
        
        let hex_str = &final_contribution.powers_of_tau.g1_powers[i];
        let clean_hex = hex_str.trim_start_matches("0x");
        let bytes = hex::decode(clean_hex)?;
        let point = G1::deserialize_compressed(&bytes[..])?;
        crs_g1.push(point);
    }
    
    // Extract G2 powers (we need degree + 1 powers)
    let mut crs_g2 = Vec::new();
    for i in 0..=degree {
        if i >= final_contribution.powers_of_tau.g2_powers.len() {
            return Err(format!("Ceremony only has {} G2 powers, but {} are needed", 
                final_contribution.powers_of_tau.g2_powers.len(), degree + 1).into());
        }
        
        let hex_str = &final_contribution.powers_of_tau.g2_powers[i];
        let clean_hex = hex_str.trim_start_matches("0x");
        let bytes = hex::decode(clean_hex)?;
        let point = G2::deserialize_compressed(&bytes[..])?;
        crs_g2.push(point);
    }
    
    Ok((crs_g1, crs_g2))
}

// helper function for polynomial addition
pub fn add<E:Field>(p1: &[E], p2: &[E]) -> Vec<E> {
    let mut result = vec![E::ZERO; std::cmp::max(p1.len(), p2.len())];

    for (i, &coeff) in p1.iter().enumerate() {
        result[i] += coeff;
    }
    for (i, &coeff) in p2.iter().enumerate() {
        result[i] += coeff;
    }

    result
}

// helper function for polynomial multiplication
pub fn mul<E:Field>(p1: &[E], p2: &[E]) -> Vec<E> {
    let mut result = vec![E::ZERO; p1.len() + p2.len() - 1];

    for (i, &coeff1) in p1.iter().enumerate() {
        for (j, &coeff2) in p2.iter().enumerate() {
            result[i + j] += coeff1 * coeff2;
        }
    }

    result
}

// helper function for polynomial division
pub fn div<E:Field>(p1: &[E], p2: &[E]) -> Result<Vec<E>, &'static str> {
    if p2.is_empty() || p2.iter().all(|&x| x == E::ZERO) {
        return Err("Cannot divide by zero polynomial");
    }

    if p1.len() < p2.len() {
        return Ok(vec![E::ZERO]);
    }

    let mut quotient = vec![E::ZERO; p1.len() - p2.len() + 1];
    let mut remainder: Vec<E> = p1.to_vec();

    while remainder.len() >= p2.len() {
        let coeff = *remainder.last().unwrap() / *p2.last().unwrap();
        let pos = remainder.len() - p2.len();

        quotient[pos] = coeff;

        for (i, &factor) in p2.iter().enumerate() {
            remainder[pos + i] -= factor * coeff;
        }

        while let Some(true) = remainder.last().map(|x| *x == E::ZERO) {
            remainder.pop();
        }
    }

    Ok(quotient)
}

// helper function to evaluate polynomial at a point
pub fn evaluate<E:Field>(poly: &[E], point: E) -> E {
    let mut value = E::ZERO;

    for i in 0..poly.len() {
        value += poly[i] * point.pow(&[i as u64]);
    }

    value
}


// helper function to perform Lagrange interpolation given a set of points
pub fn interpolate<E:Field>(points: &[E], values: &[E]) -> Result<Vec<E>, &'static str> {
    if points.len() != values.len() {
        return Err("Number of points and values do not match");
    }

    let mut result = vec![E::ZERO; points.len()];

    for i in 0..points.len() {
        let mut numerator = vec![E::ONE];
        let mut denominator = E::ONE;

        for j in 0..points.len() {
            if i == j {
                continue;
            }

            numerator = mul(&numerator, &[-points[j], E::ONE]);
            denominator *= points[i] - points[j];
        }

        let denominator_inv = denominator.inverse().unwrap();
        let term: Vec<E> = numerator.iter().map(|&x| x * values[i] * denominator_inv).collect();

        result = add(&result, &term);
    }

    Ok(result)
}

// helper function to get the roots of unity of a polynomial
#[allow(dead_code)]
pub fn get_omega<E:PrimeField>(coefficients: &[E]) -> E {
    let mut coefficients = coefficients.to_vec();
    let n = coefficients.len() - 1;
    if !n.is_power_of_two() {
        let num_coeffs = coefficients.len().checked_next_power_of_two().unwrap();
        // pad the coefficients with zeros to the nearest power of two
        for i in coefficients.len()..num_coeffs {
            coefficients[i] = E::ZERO;
        }
    }

    let m = coefficients.len();
    let exp = log2(m);
    let mut omega = E::TWO_ADIC_ROOT_OF_UNITY;
    for _ in exp..E::TWO_ADICITY {
        omega.square_in_place();
    }
    omega
}

// helper function to multiple a polynomial with a scalar value
#[allow(dead_code)]
pub fn scalar_mul<E:Field>(poly: &[E], scalar: E) -> Vec<E> {
    let mut result = Vec::with_capacity(poly.len());
    for coeff in poly {
        result.push(*coeff * scalar);
    }
    result    
}