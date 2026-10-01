//! Funkcje AI Dziwaka działające lokalnie na CPU: usuwanie tła modelem U²-Net-p (ONNX, tract).
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tract_onnx::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("nie można wczytać modelu: {0}")]
    Load(String),
    #[error("błąd obliczeń modelu: {0}")]
    Run(String),
    #[error("złe wymiary obrazu")]
    BadInput,
}

pub const INPUT_SIZE: usize = 320;

const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

/// Ścieżka modelu: pierwszy istniejący z $XDG_DATA_HOME (lub ~/.local/share) i $XDG_DATA_DIRS
/// (domyślnie /usr/local/share:/usr/share) + /dziwak/models/u2netp.onnx; gdy żaden nie istnieje — ścieżka użytkownika.
pub fn default_model_path() -> Option<PathBuf> {
    let rel = "dziwak/models/u2netp.onnx";
    let user = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|d| d.join(rel));
    // Katalogi systemowe (pakiet dystrybucji instaluje model do /usr/share/dziwak/models)
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    let system = data_dirs
        .split(':')
        .filter(|d| !d.is_empty())
        .map(|d| PathBuf::from(d).join(rel));
    // Obok programu (wersja przenośna, np. Windows z pendrive'a): <katalog exe>/models/u2netp.onnx
    let portable = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("models/u2netp.onnx")));
    let appdata = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join(rel));
    user.clone()
        .into_iter()
        .chain(portable)
        .chain(appdata)
        .chain(system)
        .find(|p| p.exists())
        .or(user)
}

pub struct BgRemover {
    model: Arc<TypedRunnableModel>,
}

impl BgRemover {
    pub fn load(path: &Path) -> Result<Self, AiError> {
        let model = tract_onnx::onnx()
            .model_for_path(path)
            .and_then(|m| m.with_input_fact(0, f32::fact([1, 3, INPUT_SIZE, INPUT_SIZE]).into()))
            .and_then(|m| m.into_optimized())
            .and_then(|m| m.into_runnable())
            .map_err(|e| AiError::Load(e.to_string()))?;
        Ok(Self { model })
    }

    /// Maska pierwszego planu (0..=255, długość w*h) dla obrazu RGBA (NIE premultiplied) w*h.
    pub fn foreground_mask(
        &self,
        rgba: &[[u8; 4]],
        w: usize,
        h: usize,
    ) -> Result<Vec<u8>, AiError> {
        if w == 0 || h == 0 || rgba.len() != w * h {
            return Err(AiError::BadInput);
        }
        let input = preprocess(rgba, w, h);
        let tensor: Tensor =
            tract_ndarray::Array4::from_shape_vec((1, 3, INPUT_SIZE, INPUT_SIZE), input)
                .map_err(|e| AiError::Run(e.to_string()))?
                .into();
        let out = self
            .model
            .run(tvec!(tensor.into()))
            .map_err(|e| AiError::Run(e.to_string()))?;
        let pred_tensor = out[0].clone().into_tensor();
        let view = pred_tensor
            .to_plain_array_view::<f32>()
            .map_err(|e| AiError::Run(e.to_string()))?;
        let pred: Vec<f32> = view.iter().copied().collect();
        if pred.len() != INPUT_SIZE * INPUT_SIZE {
            return Err(AiError::Run("zły rozmiar wyjścia".into()));
        }
        Ok(postprocess(&pred, w, h))
    }
}

/// Próbka dwuliniowa kanału `c` obrazu w*h (4 kanały u8) w punkcie (fx, fy) we współrzędnych pikseli.
fn sample_bilinear(rgba: &[[u8; 4]], w: usize, h: usize, fx: f32, fy: f32, c: usize) -> f32 {
    let fx = fx.clamp(0.0, (w - 1) as f32);
    let fy = fy.clamp(0.0, (h - 1) as f32);

    let x0 = fx.floor() as usize;
    let y0 = fy.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);

    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;

    let v00 = rgba[y0 * w + x0][c] as f32;
    let v01 = rgba[y0 * w + x1][c] as f32;
    let v10 = rgba[y1 * w + x0][c] as f32;
    let v11 = rgba[y1 * w + x1][c] as f32;

    (1.0 - tx) * (1.0 - ty) * v00 + tx * (1.0 - ty) * v10 + (1.0 - tx) * ty * v01 + tx * ty * v11
}

/// Wejście sieci NCHW 1x3x320x320: skalowanie dwuliniowe, podział przez maksimum RGB obrazu, normalizacja MEAN/STD.
pub fn preprocess(rgba: &[[u8; 4]], w: usize, h: usize) -> Vec<f32> {
    let maxv = rgba
        .iter()
        .flat_map(|p| p[..3].iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1) as f32;

    let mut out = vec![0.0f32; 3 * INPUT_SIZE * INPUT_SIZE];

    for oy in 0..INPUT_SIZE {
        for ox in 0..INPUT_SIZE {
            let fx = (ox as f32 + 0.5) * w as f32 / INPUT_SIZE as f32 - 0.5;
            let fy = (oy as f32 + 0.5) * h as f32 / INPUT_SIZE as f32 - 0.5;

            for c in 0..3 {
                let v = sample_bilinear(rgba, w, h, fx, fy, c) / maxv;
                out[c * INPUT_SIZE * INPUT_SIZE + oy * INPUT_SIZE + ox] = (v - MEAN[c]) / STD[c];
            }
        }
    }

    out
}

/// Wyjście sieci 320x320 -> maska w*h: normalizacja min-max, skalowanie dwuliniowe, 0..=255.
pub fn postprocess(pred: &[f32], w: usize, h: usize) -> Vec<u8> {
    let n = INPUT_SIZE;
    if pred.len() != n * n || w == 0 || h == 0 {
        return vec![0; w * h];
    }
    let (mi, ma) = pred
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), &p| (a.min(p), b.max(p)));
    if ma - mi < 1e-6 {
        return vec![0; w * h];
    }
    let scale = 1.0 / (ma - mi);
    let max_c = (n - 1) as f32;
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * n as f32 / h as f32 - 0.5).clamp(0.0, max_c);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(n - 1);
        let ty = fy - y0 as f32;
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * n as f32 / w as f32 - 0.5).clamp(0.0, max_c);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(n - 1);
            let tx = fx - x0 as f32;
            let top = pred[y0 * n + x0] * (1.0 - tx) + pred[y0 * n + x1] * tx;
            let bottom = pred[y1 * n + x0] * (1.0 - tx) + pred[y1 * n + x1] * tx;
            let p = top * (1.0 - ty) + bottom * ty;
            out.push((((p - mi) * scale) * 255.0).round().clamp(0.0, 255.0) as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_white_image() {
        let white_image = [[255u8; 4]; 100];
        let result = preprocess(&white_image, 10, 10);
        assert_eq!(result.len(), 3 * INPUT_SIZE * INPUT_SIZE);
        let expected = (1.0 - MEAN[0]) / STD[0];
        assert!((result[0] - expected).abs() < 1e-4);
    }

    #[test]
    fn test_postprocess_constant() {
        let constant = vec![0.5; INPUT_SIZE * INPUT_SIZE];
        let result = postprocess(&constant, 10, 10);
        assert_eq!(result.len(), 100);
        assert!(result.iter().all(|&x| x == 0));
    }

    #[test]
    fn test_postprocess_half_zero_half_one() {
        // lewa połowa kolumn 0.0, prawa 1.0
        let pred: Vec<f32> = (0..INPUT_SIZE * INPUT_SIZE)
            .map(|i| {
                if i % INPUT_SIZE >= INPUT_SIZE / 2 {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();
        let result = postprocess(&pred, 100, 10);
        assert_eq!(result[0], 0);
        assert_eq!(result[99], 255);
    }

    #[test]
    #[ignore]
    fn model_smoke() {
        if let Some(path) = default_model_path() {
            if path.exists() {
                let remover = BgRemover::load(&path).unwrap();

                // Create a white image with a black circle in the center
                let mut rgba = vec![[255u8; 4]; 64 * 64];
                let center: i32 = 32;
                let radius: i32 = 10;
                for y in 0..64 {
                    for x in 0..64 {
                        if (x as i32 - center).pow(2) + (y as i32 - center).pow(2) <= radius.pow(2)
                        {
                            rgba[y * 64 + x] = [0, 0, 0, 255];
                        }
                    }
                }

                let mask = remover.foreground_mask(&rgba, 64, 64).unwrap();
                assert!(mask[32 * 64 + 32] > mask[0]); // Center should have higher value than corner
            }
        }
    }
}
