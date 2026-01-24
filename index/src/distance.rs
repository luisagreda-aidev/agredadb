//! Distance calculations with SIMD optimization

use wide::f32x8;

/// Distance metric trait
pub trait Distance {
    fn distance(&self, a: &[f32], b: &[f32]) -> f32;
}

/// Cosine similarity (1 - cosine distance)
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same length");
    
    let len = a.len();
    
    // SIMD optimization for vectors divisible by 8
    if len >= 8 && len % 8 == 0 {
        simd_cosine_similarity(a, b)
    } else {
        scalar_cosine_similarity(a, b)
    }
}

/// SIMD-optimized cosine similarity
fn simd_cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = f32x8::ZERO;
    let mut norm_a = f32x8::ZERO;
    let mut norm_b = f32x8::ZERO;
    
    let chunks = a.len() / 8;
    
    for i in 0..chunks {
        let offset = i * 8;
        let va = f32x8::new([
            a[offset], a[offset+1], a[offset+2], a[offset+3],
            a[offset+4], a[offset+5], a[offset+6], a[offset+7],
        ]);
        let vb = f32x8::new([
            b[offset], b[offset+1], b[offset+2], b[offset+3],
            b[offset+4], b[offset+5], b[offset+6], b[offset+7],
        ]);
        
        dot += va * vb;
        norm_a += va * va;
        norm_b += vb * vb;
    }
    
    let dot_sum: f32 = dot.as_array_ref().iter().sum();
    let norm_a_sum: f32 = norm_a.as_array_ref().iter().sum();
    let norm_b_sum: f32 = norm_b.as_array_ref().iter().sum();
    
    if norm_a_sum == 0.0 || norm_b_sum == 0.0 {
        return 0.0;
    }
    
    dot_sum / (norm_a_sum.sqrt() * norm_b_sum.sqrt())
}

/// Scalar cosine similarity
fn scalar_cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

/// Euclidean distance
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same length");
    
    let mut sum = 0.0;
    for i in 0..a.len() {
        let diff = a[i] - b[i];
        sum += diff * diff;
    }
    
    sum.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![3.0, 4.0, 0.0];
        
        let dist = euclidean_distance(&a, &b);
        assert!((dist - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_simd_vs_scalar() {
        let a: Vec<f32> = (0..128).map(|i| i as f32).collect();
        let b: Vec<f32> = (0..128).map(|i| (i + 1) as f32).collect();
        
        let simd_result = simd_cosine_similarity(&a, &b);
        let scalar_result = scalar_cosine_similarity(&a, &b);
        
        assert!((simd_result - scalar_result).abs() < 1e-4);
    }
}
