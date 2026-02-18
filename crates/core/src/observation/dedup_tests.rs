//! Tests for cosine_similarity and related dedup helpers.

#[cfg(test)]
mod tests {
    use crate::cosine_similarity;

    /// Tolerance for computed floating-point results (dot products, sqrt).
    /// Empirically, f32 cosine similarity on small vectors stays within 1e-4.
    const COMPUTED_EPSILON: f32 = 1e-4;

    /// Tolerance for cases where the function returns exactly 0.0
    /// (NaN, Inf, empty, mismatched-length inputs trigger early return).
    const EXACT_ZERO_EPSILON: f32 = f32::EPSILON;

    #[test]
    fn identical_vectors_returns_1() {
        let v = vec![1.0_f32, 2.0, 3.0];
        let result = cosine_similarity(&v, &v);
        assert!((result - 1.0).abs() < COMPUTED_EPSILON, "expected ≈1.0, got {result}");
    }

    #[test]
    fn orthogonal_vectors_returns_0() {
        let a = vec![1.0_f32, 0.0, 0.0];
        let b = vec![0.0_f32, 1.0, 0.0];
        let result = cosine_similarity(&a, &b);
        assert!(result.abs() < COMPUTED_EPSILON, "expected ≈0.0, got {result}");
    }

    #[test]
    fn opposite_vectors_returns_negative() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![-1.0_f32, 0.0];
        let result = cosine_similarity(&a, &b);
        assert!((result - (-1.0)).abs() < COMPUTED_EPSILON, "expected ≈-1.0, got {result}");
    }

    #[test]
    fn empty_vectors_returns_0() {
        let result = cosine_similarity(&[], &[]);
        assert!(result.abs() < EXACT_ZERO_EPSILON, "expected 0.0, got {result}");
    }

    #[test]
    fn mismatched_length_returns_0() {
        let a = vec![1.0_f32, 2.0];
        let b = vec![1.0_f32, 2.0, 3.0];
        let result = cosine_similarity(&a, &b);
        assert!(
            result.abs() < EXACT_ZERO_EPSILON,
            "expected 0.0 for mismatched lengths, got {result}"
        );
    }

    #[test]
    fn zero_vectors_returns_0() {
        let a = vec![0.0_f32, 0.0, 0.0];
        let b = vec![0.0_f32, 0.0, 0.0];
        let result = cosine_similarity(&a, &b);
        assert!(result.abs() < EXACT_ZERO_EPSILON, "expected 0.0 for zero vectors, got {result}");
    }

    #[test]
    fn partial_similarity() {
        // cos([1,1,0], [1,0,0]) = 1 / (sqrt(2) * 1) ≈ 0.7071
        let a = vec![1.0_f32, 1.0, 0.0];
        let b = vec![1.0_f32, 0.0, 0.0];
        let result = cosine_similarity(&a, &b);
        let expected = 1.0_f32 / 2.0_f32.sqrt();
        assert!((result - expected).abs() < COMPUTED_EPSILON, "expected ≈{expected}, got {result}");
    }

    #[test]
    fn nan_input_returns_0() {
        let a = vec![f32::NAN, 1.0];
        let b = vec![1.0, 1.0];
        let result = cosine_similarity(&a, &b);
        assert!(result.abs() < EXACT_ZERO_EPSILON, "expected 0.0 for NaN input, got {result}");
    }

    #[test]
    fn infinity_input_returns_0() {
        let a = vec![f32::INFINITY, 1.0];
        let b = vec![1.0, 1.0];
        let result = cosine_similarity(&a, &b);
        assert!(result.abs() < EXACT_ZERO_EPSILON, "expected 0.0 for infinity input, got {result}");
    }

    #[test]
    fn neg_infinity_input_returns_0() {
        let a = vec![1.0, 1.0];
        let b = vec![f32::NEG_INFINITY, 0.0];
        let result = cosine_similarity(&a, &b);
        assert!(
            result.abs() < EXACT_ZERO_EPSILON,
            "expected 0.0 for -infinity input, got {result}"
        );
    }

    // ─── Property: cosine_similarity is scale-invariant ─────────────
    // cos(α·v, v) == cos(v, v) for any positive α
    #[test]
    fn scaling_invariance_property() {
        let v = vec![1.0_f32, 2.0, 3.0];
        let scaled: Vec<f32> = v.iter().map(|x| x * 42.0).collect();
        let sim_identical = cosine_similarity(&v, &v);
        let sim_scaled = cosine_similarity(&v, &scaled);
        assert!(
            (sim_identical - sim_scaled).abs() < COMPUTED_EPSILON,
            "cosine_similarity must be scale-invariant: identity={sim_identical}, scaled={sim_scaled}"
        );
    }

    // ─── Property: cosine_similarity is commutative ─────────────────
    // cos(a, b) == cos(b, a)
    #[test]
    fn commutativity_property() {
        let a = vec![1.0_f32, 3.0, 0.5];
        let b = vec![2.0_f32, 0.0, 4.0];
        let ab = cosine_similarity(&a, &b);
        let ba = cosine_similarity(&b, &a);
        assert!(
            (ab - ba).abs() < EXACT_ZERO_EPSILON,
            "cosine_similarity must be commutative: ({a:?},{b:?})={ab} vs ({b:?},{a:?})={ba}"
        );
    }

    // ─── Property: cosine_similarity bounded [-1, 1] ────────────────
    #[test]
    fn bounded_output_property() {
        let test_vectors: Vec<Vec<f32>> = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![-1.0, -1.0, -1.0],
            vec![100.0, 0.001, -50.0],
            vec![f32::MIN_POSITIVE, f32::MIN_POSITIVE, f32::MIN_POSITIVE],
        ];
        for a in &test_vectors {
            for b in &test_vectors {
                let sim = cosine_similarity(a, b);
                assert!(
                    (-1.0 - COMPUTED_EPSILON..=1.0 + COMPUTED_EPSILON).contains(&sim),
                    "cosine_similarity({a:?}, {b:?}) = {sim} is out of [-1, 1] bounds"
                );
            }
        }
    }

    // ─── Property: contains_non_finite catches all non-finite floats ─
    #[test]
    fn contains_non_finite_all_variants() {
        use crate::contains_non_finite;

        assert!(contains_non_finite(&[f32::NAN]), "NaN");
        assert!(contains_non_finite(&[f32::INFINITY]), "INFINITY");
        assert!(contains_non_finite(&[f32::NEG_INFINITY]), "NEG_INFINITY");
        assert!(contains_non_finite(&[1.0, f32::NAN, 3.0]), "NaN in middle");
        assert!(contains_non_finite(&[1.0, 2.0, f32::INFINITY]), "INFINITY at end");

        assert!(!contains_non_finite(&[]), "empty");
        assert!(!contains_non_finite(&[0.0]), "zero");
        assert!(!contains_non_finite(&[f32::MIN_POSITIVE]), "MIN_POSITIVE");
        assert!(!contains_non_finite(&[f32::MAX]), "MAX");
        assert!(!contains_non_finite(&[f32::MIN]), "MIN");
        assert!(!contains_non_finite(&[-0.0]), "negative zero");

        // Subnormal numbers are finite and should pass
        let subnormal = f32::MIN_POSITIVE / 2.0;
        assert!(!contains_non_finite(&[subnormal]), "subnormal must be finite");
    }

    // ─── Property: is_zero_vector identity ──────────────────────────
    #[test]
    fn is_zero_vector_edge_cases() {
        use crate::is_zero_vector;

        assert!(is_zero_vector(&[]), "empty is vacuously all-zero");
        assert!(is_zero_vector(&[0.0, 0.0, 0.0]), "all zeros");
        assert!(!is_zero_vector(&[0.0, f32::MIN_POSITIVE, 0.0]), "MIN_POSITIVE is not zero");
        assert!(!is_zero_vector(&[0.0, -0.0, f32::MIN_POSITIVE]), "includes non-zero");

        // -0.0 == 0.0 in IEEE 754, so vec of -0.0 IS a zero vector
        assert!(is_zero_vector(&[-0.0, -0.0]), "negative zeros are zeros");
    }

    // ─── Metamorphic: negation flips sign ───────────────────────────
    // cos(-a, b) == -cos(a, b) for non-degenerate vectors
    #[test]
    fn negation_flips_similarity() {
        let a = vec![1.0_f32, 2.0, 3.0];
        let b = vec![4.0_f32, 5.0, 6.0];
        let neg_a: Vec<f32> = a.iter().map(|x| -x).collect();

        let sim_ab = cosine_similarity(&a, &b);
        let sim_neg_ab = cosine_similarity(&neg_a, &b);

        assert!(
            (sim_ab + sim_neg_ab).abs() < COMPUTED_EPSILON,
            "cos(-a,b) should equal -cos(a,b): {sim_neg_ab} vs -{sim_ab}"
        );
    }
}
