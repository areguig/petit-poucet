use std::fmt;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    Info,
    Warning,
    Error,
}

pub struct Item {
    pub label: String,
    pub status: Status,
    pub message: String,
    pub fix: Option<String>,
}

impl Item {
    pub fn new(
        label: impl Into<String>,
        status: Status,
        message: impl Into<String>,
        fix: Option<String>,
    ) -> Self {
        Self {
            label: label.into(),
            status,
            message: message.into(),
            fix,
        }
    }
}

fn safe(text: &str) -> String {
    if crate::secrets::find(text).is_some() {
        return "[redacted]".into();
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl fmt::Display for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = match self.status {
            Status::Ok => "ok",
            Status::Info => "info",
            Status::Warning => "warning",
            Status::Error => "error",
        };
        write!(
            f,
            "{}: {status}: {}",
            safe(&self.label),
            safe(&self.message)
        )?;
        if let Some(fix) = &self.fix {
            write!(f, "; run `{}`", safe(fix))?;
        }
        Ok(())
    }
}

pub fn exit_code(items: &[Item]) -> u8 {
    if items.iter().any(|i| i.status == Status::Error) {
        1
    } else if items.iter().any(|i| i.status == Status::Warning) {
        2
    } else {
        0
    }
}

pub fn quote(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        format!("'{}'", text.replace('\'', "''"))
    } else {
        shlex::try_quote(&text)
            .expect("filesystem paths contain no NUL")
            .into_owned()
    }
}

pub fn edit(path: &Path) -> String {
    format!(
        "{} {}",
        if cfg!(windows) { "notepad" } else { "vi" },
        quote(path)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_take_precedence_over_warnings_and_info_is_success() {
        let mut items = vec![Item::new("release", Status::Info, "offline", None)];
        assert_eq!(exit_code(&items), 0);
        items.push(Item::new(
            "release",
            Status::Warning,
            "newer version",
            Some("petit-poucet update".into()),
        ));
        assert_eq!(exit_code(&items), 2);
        items.push(Item::new(
            "config",
            Status::Error,
            "missing",
            Some("petit-poucet setup".into()),
        ));
        assert_eq!(exit_code(&items), 1);
    }

    #[test]
    fn reports_are_single_lines_and_never_repeat_a_secret() {
        let item = Item::new(
            "config\nvalue",
            Status::Error,
            "token=synthetic-secret-value",
            Some("vi\nconfig.toml".into()),
        );
        assert_eq!(
            item.to_string(),
            "config value: error: [redacted]; run `vi config.toml`"
        );
    }
}
