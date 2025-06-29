use std::ops::Mul;
use ark_ff::Field;
use ark_ec::pairing::Pairing;
use crate::utils::{div, mul, evaluate, interpolate};

#[allow(clippy::upper_case_acronyms)]
pub struct KZG<E: Pairing> {
    pub g1: E::G1,
    pub g2: E::G2,
    pub g2_tau: E::G2,
    pub degree: usize,
    pub crs_g1: Vec<E::G1>,
    pub crs_g2: Vec<E::G2>,
}


impl <E:Pairing> KZG<E> {
    #[allow(dead_code)]
    pub fn new(g1: E::G1, g2: E::G2, degree: usize) -> Self {
        Self {
            g1,
            g2,
            g2_tau: g2.mul(E::ScalarField::default()),
            degree,
            crs_g1: vec![],
            crs_g2: vec![],
        }
    }

    /// Create KZG instance with pre-computed CRS from trusted setup ceremony
    pub fn from_trusted_setup(
        g1: E::G1, 
        g2: E::G2, 
        degree: usize,
        crs_g1: Vec<E::G1>,
        crs_g2: Vec<E::G2>
    ) -> Result<Self, &'static str> {
        if crs_g1.len() < degree + 1 {
            return Err("CRS G1 powers insufficient for degree");
        }
        if crs_g2.len() < degree + 1 {
            return Err("CRS G2 powers insufficient for degree");
        }
        
        // g2_tau is the first power of tau in G2 (index 1)
        let g2_tau = if crs_g2.len() > 1 {
            crs_g2[1]
        } else {
            return Err("CRS G2 needs at least 2 powers for g2_tau");
        };
        
        Ok(Self {
            g1,
            g2,
            g2_tau,
            degree,
            crs_g1,
            crs_g2,
        })
    }

    #[allow(dead_code)]
    pub fn setup(&mut self, secret: E::ScalarField) {
        for i in 0..self.degree+1 {
            self.crs_g1.push(self.g1.mul(secret.pow([i as u64])));
            self.crs_g2.push(self.g2.mul(secret.pow([i as u64])));
        }
        self.g2_tau = self.g2.mul(secret);
    }

    pub fn commit(&self, poly: &[E::ScalarField]) -> E::G1 {
        let mut commitment = self.g1.mul(E::ScalarField::default());
        for (i, coeff) in poly.iter().enumerate().take(self.degree+1) {
            commitment += self.crs_g1[i] * coeff;
        }
        commitment
    }

    pub fn open(&self, poly: &[E::ScalarField], point: E::ScalarField) -> E::G1 {
        // evaluate the polynomial at point
        let value = evaluate(poly, point);

        // initialize denominator
        let denominator = [-point, E::ScalarField::ONE];

        // initialize numerator
        let first = poly[0] - value;
        let rest = &poly[1..];
        let temp: Vec<E::ScalarField> = std::iter::once(first).chain(rest.iter().cloned()).collect();
        let numerator: &[E::ScalarField] = &temp;

        // get quotient by dividing numerator by denominator
        let quotient = div(numerator, &denominator).unwrap();

        // calculate pi as proof (quotient multiplied by CRS)
        let mut pi = self.g1.mul(E::ScalarField::default());
        for (i, quo) in quotient.iter().enumerate() {
            pi += self.crs_g1[i] * quo;
        }

        // return pi
        pi
    }

    pub fn multi_open(&self, poly: &[E::ScalarField], points: &[E::ScalarField]) -> E::G1 {
        // denominator is a polynomial where all its root are points to be evaluated (zero poly)
        let mut zero_poly = vec![-points[0], E::ScalarField::ONE];
        for point in points.iter().skip(1) {
            zero_poly = mul(&zero_poly, &[-*point, E::ScalarField::ONE]);
        }

        // perform Lagrange interpolation on points
        let mut values = vec![];
        for point in points {
            values.push(evaluate(poly, *point));
        }
        let mut lagrange_poly = interpolate(points, &values).unwrap();
        lagrange_poly.resize(poly.len(), E::ScalarField::default()); // pad with zeros

        // numerator is the difference between the polynomial and the Lagrange interpolation
        let mut numerator = Vec::with_capacity(poly.len());
        for (coeff1, coeff2) in poly.iter().zip(lagrange_poly.as_slice()) {
            numerator.push(*coeff1 - coeff2);
        }

        // get quotient by dividing numerator by denominator
        let quotient = div(&numerator, &zero_poly).unwrap();

        // calculate pi as proof (quotient multiplied by CRS)
        let mut pi = self.g1.mul(E::ScalarField::default());
        for (i, quo) in quotient.iter().enumerate() {
            pi += self.crs_g1[i] * *quo;
        }

        // return pi
        pi
    }

    pub fn verify(
        &self,
        point: E::ScalarField,
        value: E::ScalarField,
        commitment: E::G1,
        pi: E::G1
    ) -> bool {
        let lhs = E::pairing(pi, self.g2_tau - self.g2.mul(point));
        let rhs = E::pairing(commitment - self.g1.mul(value), self.g2);
        lhs == rhs
    }

    pub fn verify_multi(
        &self,
        points: &[E::ScalarField],
        values: &[E::ScalarField],
        commitment: E::G1,
        pi: E::G1
    ) -> bool {
        // compute the zero polynomial
        let mut zero_poly = vec![-points[0], E::ScalarField::ONE];
        for point in points.iter().skip(1) {
            zero_poly = mul(&zero_poly, &[-*point, E::ScalarField::ONE]);
        }

        // compute commitment of zero polynomial in regards to crs_g2
        let mut zero_commitment = self.g2.mul(E::ScalarField::default());
        for (i, coeff) in zero_poly.iter().enumerate().take(self.crs_g2.len()) {
            zero_commitment += self.crs_g2[i] * coeff;
        }

        // compute lagrange polynomial
        let lagrange_poly = interpolate(points, values).unwrap();

        // compute commitment of lagrange polynomial in regards to crs_g1
        let mut lagrange_commitment = self.g1.mul(E::ScalarField::default());
        for (i, coeff) in lagrange_poly.iter().enumerate().take(std::cmp::min(lagrange_poly.len(), self.crs_g1.len())) {
            lagrange_commitment += self.crs_g1[i] * coeff;
        }

        let lhs = E::pairing(pi, zero_commitment);
        let rhs = E::pairing(commitment - lagrange_commitment, self.g2);
        lhs == rhs
    }
}