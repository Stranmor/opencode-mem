//! Tests for cosine_similarity and related dedup helpers.

#[expect(clippy::unwrap_used, reason = "test code")]
#[cfg(test)]
mod tests {
    use crate::cosine_similarity;

    #[test]
    fn identical_vectors_returns_1() {
        let v = vec![1.0_f32, 2.0, 3.0];
        let result = cosine_similarity(&v, &v);
        assert!((result - 1.0).abs() < 0.001, "expected ≈1.0, got {result}");
    }

    #[test]
    fn orthogonal_vectors_returns_0() {
        let a = vec![1.0_f32, 0.0, 0.0];
        let b = vec![0.0_f32, 1.0, 0.0];
        let result = cosine_similarity(&a, &b);
        assert!(result.abs() < 0.001, "expected ≈0.0, got {result}");
    }

    #[test]
    fn opposite_vectors_returns_negative() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![-1.0_f32, 0.0];
        let result = cosine_similarity(&a, &b);
        assert!((result - (-1.0)).abs() < 0.001, "expected ≈-1.0, got {result}");
    }

    #[test]
    fn empty_vectors_returns_0() {
        let result = cosine_similarity(&[], &[]);
        assert!(result.abs() < f32::EPSILON, "expected 0.0, got {result}");
    }

    #[test]
    fn mismatched_length_returns_0() {
        let a = vec![1.0_f32, 2.0];
        let b = vec![1.0_f32, 2.0, 3.0];
        let result = cosine_similarity(&a, &b);
        assert!(result.abs() < f32::EPSILON, "expected 0.0 for mismatched lengths, got {result}");
    }

    #[test]
    fn zero_vectors_returns_0() {
        let a = vec![0.0_f32, 0.0, 0.0];
        let b = vec![0.0_f32, 0.0, 0.0];
        let result = cosine_similarity(&a, &b);
        assert!(result.abs() < f32::EPSILON, "expected 0.0 for zero vectors, got {result}");
    }

    #[test]
    fn partial_similarity() {
        // cos([1,1,0], [1,0,0]) = 1 / (sqrt(2) * 1) ≈ 0.7071
        let a = vec![1.0_f32, 1.0, 0.0];
        let b = vec![1.0_f32, 0.0, 0.0];
        let result = cosine_similarity(&a, &b);
        let expected = 1.0_f32 / 2.0_f32.sqrt();
        assert!((result - expected).abs() < 0.001, "expected ≈{expected}, got {result}");
    }
}
