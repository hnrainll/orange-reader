use crate::store::Store;
use serde_json::{Value, json};

#[derive(Clone)]
pub struct ReaderSettings {
    pub font_size: f32,
    pub line_height: f32,
    pub content_width: f32,
    pub vertical_margin: f32,
    pub letter_spacing: f32,
    pub paragraph_spacing: f32,
    pub font_family: String,
    pub theme: String,
    pub spread: String,
}

pub fn is_valid_theme(theme: &str) -> bool {
    matches!(theme, "light" | "sepia" | "gray" | "dark")
}

pub fn theme_bg(theme: &str) -> u32 {
    match theme {
        "sepia" => 0xf8f1e3,
        "gray" => 0x49494c,
        "dark" => 0x161618,
        _ => 0xffffff,
    }
}

impl ReaderSettings {
    pub fn load(store: &Store) -> Self {
        let number = |key: &str, default: f32, min: f32, max: f32| {
            store
                .setting(key)
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .unwrap_or(default)
                .clamp(min, max)
        };
        Self {
            font_size: number("font_size", 18.0, 12.0, 32.0),
            line_height: number("line_height", 1.65, 1.2, 2.4),
            content_width: number("content_width", 760.0, 360.0, 1600.0),
            vertical_margin: number("vertical_margin", 26.0, 12.0, 140.0),
            letter_spacing: number("letter_spacing", 0.0, 0.0, 0.1),
            paragraph_spacing: number("paragraph_spacing", 1.0, 0.5, 2.4),
            font_family: store
                .setting("font_family")
                .filter(|family| matches!(family.as_str(), "original" | "serif" | "sans-serif"))
                .unwrap_or_else(|| "original".to_owned()),
            theme: store
                .setting("theme")
                .filter(|theme| is_valid_theme(theme))
                .unwrap_or_else(|| "light".to_owned()),
            spread: store
                .setting("spread")
                .filter(|spread| matches!(spread.as_str(), "auto" | "none"))
                .unwrap_or_else(|| "auto".to_owned()),
        }
    }

    pub fn save_from_json(store: &Store, value: &Value) -> anyhow::Result<()> {
        let save_num = |json_key: &str, db_key: &str, min: f64, max: f64| -> anyhow::Result<()> {
            if let Some(num) = value
                .get(json_key)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
            {
                let clamped = num.clamp(min, max);
                store.save_setting(db_key, &format!("{clamped:.2}"))?;
            }
            Ok(())
        };
        save_num("fontSize", "font_size", 12.0, 32.0)?;
        save_num("lineHeight", "line_height", 1.2, 2.4)?;
        save_num("contentWidth", "content_width", 360.0, 1600.0)?;
        save_num("verticalMargin", "vertical_margin", 12.0, 140.0)?;
        save_num("letterSpacing", "letter_spacing", 0.0, 0.1)?;
        save_num("paragraphSpacing", "paragraph_spacing", 0.5, 2.4)?;
        if let Some(family) = value
            .get("fontFamily")
            .and_then(Value::as_str)
            .filter(|f| matches!(*f, "original" | "serif" | "sans-serif"))
        {
            store.save_setting("font_family", family)?;
        }
        if let Some(theme) = value
            .get("theme")
            .and_then(Value::as_str)
            .filter(|t| is_valid_theme(t))
        {
            store.save_setting("theme", theme)?;
        }
        if let Some(spread) = value
            .get("spread")
            .and_then(Value::as_str)
            .filter(|s| matches!(*s, "auto" | "none"))
        {
            store.save_setting("spread", spread)?;
        }
        Ok(())
    }

    pub fn json(&self) -> Value {
        json!({
            "fontSize": self.font_size,
            "lineHeight": self.line_height,
            "contentWidth": self.content_width,
            "verticalMargin": self.vertical_margin,
            "letterSpacing": self.letter_spacing,
            "paragraphSpacing": self.paragraph_spacing,
            "fontFamily": self.font_family,
            "theme": self.theme,
            "spread": self.spread,
        })
    }
}
