//! Icons from the sprite in `static/icons.svg`.

use std::fmt;

/// An icon, written in templates as `{{ Icon("home") }}`.
///
/// Icons are decorative: the text next to them carries the meaning, so
/// they are hidden from screen readers. The name is a symbol id in the
/// sprite; only string literals are accepted, so it never comes from input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Icon(pub(crate) &'static str);

impl fmt::Display for Icon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            r#"<svg class="icon" aria-hidden="true" focusable="false"><use href="/static/icons.svg#{}"></use></svg>"#,
            self.0
        )
    }
}

/// The markup is fixed apart from the literal name.
impl askama::filters::HtmlSafe for Icon {}

#[cfg(test)]
mod tests {
    /// Every icon a template or handler names exists in the sprite.
    #[test]
    fn named_icons_exist() {
        let sprite = include_str!("../../static/icons.svg");
        let sources = [
            include_str!("../../templates/base.html"),
            include_str!("../../templates/home.html"),
            include_str!("../../templates/history.html"),
            include_str!("../../templates/schedule.html"),
            include_str!("../../templates/groups.html"),
            include_str!("../../templates/devices.html"),
            include_str!("../../templates/enroll.html"),
            include_str!("../../templates/settings.html"),
            include_str!("../../templates/login.html"),
        ];
        for source in sources {
            for name in source
                .split("Icon(\"")
                .skip(1)
                .filter_map(|rest| rest.split('"').next())
            {
                assert!(
                    sprite.contains(&format!("id=\"{name}\"")),
                    "missing icon {name}"
                );
            }
        }
    }
}
