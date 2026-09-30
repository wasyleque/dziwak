//! Jądro rozmycia Gaussa.
//!
//! Funkcja:
//! Symetryczne jądro 1D o długości 2*radius+1, suma wag = 1.0.

/// Symetryczne jądro 1D o długości 2*radius+1, suma wag = 1.0.
pub fn gaussian_kernel_1d(radius: usize, sigma: f32) -> Vec<f32> {
    if radius == 0 {
        return vec![1.0];
    }

    let mut sigma = sigma;
    if sigma <= 0.0 {
        sigma = radius as f32 / 2.0;
    }
    if sigma <= 0.0 {
        return vec![1.0];
    }

    let size = 2 * radius + 1;
    let mut kernel = Vec::with_capacity(size);

    // Calculate the sum of all weights
    let mut sum = 0.0;
    for i in 0..size {
        let x = (i as i32 - radius as i32) as f32;
        let weight = (-x * x / (2.0 * sigma * sigma)).exp();
        kernel.push(weight);
        sum += weight;
    }

    // Normalize the weights
    for weight in &mut kernel {
        *weight /= sum;
    }

    kernel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gaussian_kernel_1d_radius_3() {
        let kernel = gaussian_kernel_1d(3, 1.0);
        assert_eq!(kernel.len(), 7); // 2*3+1 = 7
    }

    #[test]
    fn test_gaussian_kernel_1d_sum_approx_1() {
        let kernel = gaussian_kernel_1d(3, 1.0);
        let sum: f32 = kernel.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_gaussian_kernel_1d_symmetry() {
        let kernel = gaussian_kernel_1d(3, 1.0);
        let len = kernel.len();
        for i in 0..len / 2 {
            assert_eq!(kernel[i], kernel[len - 1 - i]);
        }
    }

    #[test]
    fn test_gaussian_kernel_1d_center_largest() {
        let kernel = gaussian_kernel_1d(3, 1.0);
        let max_val = *kernel
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        assert_eq!(kernel[3], max_val); // Center element should be the largest
    }

    #[test]
    fn test_gaussian_kernel_1d_radius_0() {
        let kernel = gaussian_kernel_1d(0, 1.0);
        assert_eq!(kernel, vec![1.0]);
    }

    #[test]
    fn test_gaussian_kernel_1d_negative_sigma() {
        let kernel = gaussian_kernel_1d(3, -1.0);
        assert_eq!(kernel.len(), 7);
        let sum: f32 = kernel.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_gaussian_kernel_1d_zero_radius_zero_sigma() {
        let kernel = gaussian_kernel_1d(0, 0.0);
        assert_eq!(kernel, vec![1.0]);
    }
}
