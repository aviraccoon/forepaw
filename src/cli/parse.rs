/// Shared parsing utilities for CLI arguments.
use crate::core::types::{Point, Rect};

/// Parse "x,y" coordinate string into a Point.
pub fn parse_coordinate(s: &str) -> Option<Point> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 2 {
        return None;
    }
    let x: f64 = parts[0].trim().parse().ok()?;
    let y: f64 = parts[1].trim().parse().ok()?;
    Some(Point::new(x, y))
}

/// Parse "x,y,w,h" into a Rect (4 components = region).
pub fn parse_region(s: &str) -> Option<Rect> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 4 {
        return None;
    }
    let x: f64 = parts[0].trim().parse().ok()?;
    let y: f64 = parts[1].trim().parse().ok()?;
    let w: f64 = parts[2].trim().parse().ok()?;
    let h: f64 = parts[3].trim().parse().ok()?;
    Some(Rect::new(x, y, w, h))
}

/// Resolve text from either positional argument or --text option.
/// Errors if neither is provided or both are provided.
pub fn resolve_text<'a>(
    positional: Option<&'a str>,
    option: Option<&'a str>,
    command: &str,
) -> anyhow::Result<&'a str> {
    match (option, positional) {
        (Some(_), Some(_)) => {
            anyhow::bail!("Provide text as either a positional argument or --text, not both.")
        }
        (Some(text), None) => Ok(text),
        (None, Some(text)) => Ok(text),
        (None, None) => {
            anyhow::bail!(
                "{} requires text. Provide as argument or use --text for text starting with dashes.",
                command
            )
        }
    }
}

/// Split a string into shell-like tokens, respecting double quotes.
pub fn shell_split(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in input.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if ch == ' ' && !in_quotes {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_coordinate_valid() {
        let p = parse_coordinate("500,300").unwrap();
        assert!((p.x - 500.0).abs() < f64::EPSILON);
        assert!((p.y - 300.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_coordinate_spaces() {
        let p = parse_coordinate(" 500 , 300 ").unwrap();
        assert!((p.x - 500.0).abs() < f64::EPSILON);
        assert!((p.y - 300.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_coordinate_invalid() {
        assert!(parse_coordinate("500").is_none());
        assert!(parse_coordinate("500,300,200").is_none());
        assert!(parse_coordinate("abc,def").is_none());
        assert!(parse_coordinate("").is_none());
    }

    #[test]
    fn parse_region_valid() {
        let r = parse_region("10,50,400,300").unwrap();
        assert!((r.x - 10.0).abs() < f64::EPSILON);
        assert!((r.width - 400.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_region_invalid() {
        assert!(parse_region("10,50").is_none());
        assert!(parse_region("10,50,400").is_none());
        assert!(parse_region("abc,50,400,300").is_none());
    }

    #[test]
    fn resolve_text_positional() {
        let result = resolve_text(Some("hello"), None, "test").unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn resolve_text_option() {
        let result = resolve_text(None, Some("hello"), "test").unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn resolve_text_neither() {
        assert!(resolve_text(None::<&str>, None::<&str>, "test").is_err());
    }

    #[test]
    fn resolve_text_both() {
        assert!(resolve_text(Some("a"), Some("b"), "test").is_err());
    }

    #[test]
    fn shell_split_simple() {
        assert_eq!(
            shell_split("click @e3 --app Finder"),
            vec!["click", "@e3", "--app", "Finder"]
        );
    }

    #[test]
    fn shell_split_quoted() {
        assert_eq!(
            shell_split("type @e5 \"hello world\""),
            vec!["type", "@e5", "hello world"]
        );
    }

    #[test]
    fn shell_split_empty() {
        assert!(shell_split("").is_empty());
    }

    #[test]
    fn shell_split_extra_spaces() {
        assert_eq!(
            shell_split("  click   @e3  "),
            vec!["click", "@e3"]
        );
    }
}
