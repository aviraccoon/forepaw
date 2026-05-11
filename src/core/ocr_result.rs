/// OCR result types.
use crate::core::types::Rect;

/// Recognized text with its bounding box.
#[derive(Debug, Clone)]
pub struct OCRResult {
    pub text: String,
    pub bounds: Rect,
}

impl OCRResult {
    pub fn new(text: impl Into<String>, bounds: Rect) -> Self {
        Self {
            text: text.into(),
            bounds,
        }
    }

    /// Center point of the recognized text region.
    pub fn center(&self) -> (f64, f64) {
        (
            self.bounds.x + self.bounds.width / 2.0,
            self.bounds.y + self.bounds.height / 2.0,
        )
    }
}

/// Combined OCR output: recognized text results plus optional display screenshot.
#[derive(Debug, Clone)]
pub struct OCROutput {
    pub results: Vec<OCRResult>,
    pub screenshot_path: Option<String>,
}

impl OCROutput {
    pub fn new(results: Vec<OCRResult>, screenshot_path: Option<String>) -> Self {
        Self {
            results,
            screenshot_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_result_center() {
        let r = OCRResult::new("Hello", Rect::new(10.0, 20.0, 50.0, 15.0));
        let (cx, cy) = r.center();
        assert!((cx - 35.0).abs() < f64::EPSILON);
        assert!((cy - 27.5).abs() < f64::EPSILON);
    }

    #[test]
    fn ocr_output_with_results_and_path() {
        let output = OCROutput::new(
            vec![OCRResult::new("Hello", Rect::new(10.0, 20.0, 50.0, 15.0))],
            Some("/tmp/test.jpg".into()),
        );
        assert_eq!(output.results.len(), 1);
        assert_eq!(output.screenshot_path.as_deref(), Some("/tmp/test.jpg"));
    }

    #[test]
    fn ocr_output_empty() {
        let output = OCROutput::new(Vec::new(), None);
        assert!(output.results.is_empty());
        assert!(output.screenshot_path.is_none());
    }
}
