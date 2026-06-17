//! Platform-agnostic core types and algorithms for desktop accessibility automation.
//!
//! These modules are used by all platform backends and define the shared data
//! model (element tree, OCR results, geometric types), algorithms (tree pruning,
//! snapshot diffing, ref assignment), and utilities (logging helpers, temp files).

pub mod annotation;
pub mod cast;
pub mod coordinate_validation;
pub mod crop_region;
pub mod element_tree;
pub mod encoder_detection;
pub mod errors;
pub mod icon_class_parser;
pub mod key_combo;
pub mod ocr_result;
pub mod output_formatter;
pub mod ref_assigner;
pub mod role;
pub mod signature;
pub mod snapshot_cache;
pub mod snapshot_diff;
pub mod temp;
pub mod text_attrs;
pub mod tree_pruning;
pub mod tree_renderer;
pub mod types;
