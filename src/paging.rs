pub type Item = (String, String);

// Items are (section, line). A page that continues a section repeats its heading, and a line too long for a page
// goes on over the next ones, between characters. `head` starts the first page.
// A section name longer than a quarter of the budget is shortened in its heading, so the heading never crowds out the text.
pub fn paginate(
    items: &[Item],
    budget: usize,
    head: &str,
    heading: impl Fn(&str) -> String,
) -> Vec<String> {
    let mut pages = vec![head.to_string()];
    let mut open: Option<&str> = None;
    for (section, line) in items {
        let title = heading(&shortened(section, budget / 4));
        let mut rest = line.as_str();
        loop {
            let page = pages.last_mut().unwrap();
            let starts = open != Some(section.as_str());
            let cost = if starts { title.len() } else { 0 };
            let room = budget.saturating_sub(page.len() + cost + 1);
            if (rest.len() > room || room == 0) && !page.is_empty() {
                pages.push(String::new());
                open = None;
                continue;
            }
            // One character at least, so a budget too small for anything still moves on.
            let end = rest
                .floor_char_boundary(room)
                .max(rest.ceil_char_boundary(1));
            if starts {
                page.push_str(&title);
                open = Some(section);
            }
            page.push_str(&rest[..end]);
            page.push('\n');
            rest = &rest[end..];
            if rest.is_empty() {
                break;
            }
            pages.push(String::new());
            open = None;
        }
    }
    pages
}

fn shortened(name: &str, max: usize) -> String {
    if name.len() <= max {
        return name.to_string();
    }
    format!(
        "{}…",
        &name[..name.floor_char_boundary(max.saturating_sub("…".len()))]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::PAGE_BYTES;

    fn heading(section: &str) -> String {
        format!("## {section}\n")
    }

    #[test]
    fn pages_fit_the_budget_repeat_their_heading_and_keep_every_line_in_order() {
        let items: Vec<(String, String)> = (0..150)
            .map(|i| {
                let section = if i < 100 { "Projects/big" } else { "check" };
                (
                    section.to_string(),
                    format!("- line {i} of about the length of a real one"),
                )
            })
            .collect();

        let pages = paginate(&items, PAGE_BYTES, "", heading);
        assert!(pages.len() > 2);
        let mut lines = Vec::new();
        for page in &pages {
            assert!(page.len() <= PAGE_BYTES, "{} bytes", page.len());
            assert!(page.starts_with("## "), "{page}");
            lines.extend(page.lines().filter(|l| !l.starts_with("## ")));
        }
        let expected: Vec<&str> = items.iter().map(|(_, line)| line.as_str()).collect();
        assert_eq!(lines, expected);
    }

    #[test]
    fn a_line_too_long_for_its_page_goes_on_after_the_repeated_heading_between_characters() {
        let section = format!("Projects/{}", "a".repeat(100));
        let long = format!("- x{}", "é".repeat(600));
        let items = vec![
            (section.clone(), "- short".to_string()),
            (section.clone(), long.clone()),
        ];

        let pages = paginate(&items, 600, "", heading);
        assert!(pages.len() > 2);
        let mut text = String::new();
        for page in &pages {
            assert!(page.len() <= 600, "{} bytes: {page}", page.len());
            let body = page.strip_prefix(&format!("## {section}\n")).unwrap();
            text.extend(body.lines());
        }
        assert_eq!(text, format!("- short{long}"));
    }
}
