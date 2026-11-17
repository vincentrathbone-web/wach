//! DESIGN.md §7: every design value comes from ui/tokens.css.
//! Fails if a colour, length, or time is written straight into another UI file.

use std::{fs, path::Path};

fn ui_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../ui"))
}

/// Raw design values inside a CSS value (after the property's colon).
fn raw_values(value: &str) -> Vec<String> {
    let mut found = Vec::new();
    let v = value.to_ascii_lowercase();
    if v.contains("rgb(") || v.contains("rgba(") || v.contains("hsl(") {
        found.push("rgb/hsl colour".into());
    }
    let b = v.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        // #abc / #aabbcc colours
        if c == b'#' {
            let hex = b[i + 1..].iter().take_while(|x| x.is_ascii_hexdigit()).count();
            if matches!(hex, 3 | 4 | 6 | 8) {
                found.push(format!("hex colour in `{}`", value.trim()));
            }
        }
        // a number glued to a unit: 12px, 1.5rem, 140ms, 2s, 45deg
        if c.is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let end = b[i..].iter().position(|x| !(x.is_ascii_digit() || *x == b'.')).map_or(b.len(), |p| i + p);
            let unit: String = b[end..].iter().take_while(|x| x.is_ascii_alphabetic()).map(|&x| x as char).collect();
            if matches!(unit.as_str(), "px" | "rem" | "em" | "ms" | "s" | "deg" | "pt" | "vh" | "vw") {
                found.push(format!("`{}{}` in `{}`", String::from_utf8_lossy(&b[i..end]), unit, value.trim()));
            }
        }
    }
    found
}

fn strip_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = rest[start + 2..].find("*/").map_or("", |e| &rest[start + 2 + e + 2..]);
    }
    out.push_str(rest);
    out
}

#[test]
fn css_uses_only_tokens() {
    let mut problems = Vec::new();
    for entry in fs::read_dir(ui_dir()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.extension().is_none_or(|e| e != "css") || name == "tokens.css" {
            continue;
        }
        let css = strip_comments(&fs::read_to_string(&path).unwrap());
        // Every declaration is "prop: value" between { or ; and ; or }.
        for block in css.split('{').skip(1) {
            let body = block.split('}').next().unwrap_or("");
            for decl in body.split(';') {
                if let Some((prop, value)) = decl.split_once(':') {
                    let prop = prop.trim();
                    if prop.starts_with("--ratio") {
                        continue; // layout state, not a design value
                    }
                    for p in raw_values(value) {
                        problems.push(format!("{name}: {prop}: {p}"));
                    }
                }
            }
        }
    }
    assert!(problems.is_empty(), "raw design values outside tokens.css:\n{}", problems.join("\n"));
}

#[test]
fn html_and_js_have_no_colours() {
    let mut problems = Vec::new();
    for file in ["index.html", "app.js"] {
        let text = fs::read_to_string(ui_dir().join(file)).unwrap();
        for (n, line) in text.lines().enumerate() {
            let l = line.to_ascii_lowercase();
            let hex = l.match_indices('#').any(|(i, _)| {
                let rest = &l.as_bytes()[i + 1..];
                let k = rest.iter().take_while(|x| x.is_ascii_hexdigit()).count();
                matches!(k, 3 | 6 | 8) && rest.get(k).is_none_or(|c| !c.is_ascii_alphanumeric() && *c != b'-')
            });
            if hex || l.contains("rgb(") || l.contains("rgba(") || l.contains(" style=") {
                problems.push(format!("{file}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(problems.is_empty(), "colours or inline styles outside tokens.css:\n{}", problems.join("\n"));
}

#[test]
fn checker_catches_raw_values() {
    assert!(!raw_values(" 12px").is_empty());
    assert!(!raw_values(" #fff").is_empty());
    assert!(!raw_values(" rgba(0,0,0,.5)").is_empty());
    assert!(!raw_values(" 140ms ease").is_empty());
    assert!(raw_values(" var(--space-2)").is_empty());
    assert!(raw_values(" calc(var(--space-3) * -1)").is_empty());
    assert!(raw_values(" 1fr 50% 0").is_empty());
}
