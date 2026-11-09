use crate::note::Note;

pub fn note_text(note: &Note) -> String {
    format!(
        "{}\n{}\n{}\n{}",
        note.title(),
        note.summary().unwrap_or_default(),
        note.applies_when().unwrap_or_default(),
        note.body
    )
}
