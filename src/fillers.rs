// The text's own language decides its filler words; English when the language can't be told reliably.
pub fn of(text: &str) -> &'static [&'static str] {
    whatlang::detect(text)
        .filter(|info| info.is_reliable())
        .and_then(|info| isolang::Language::from_639_3(info.lang().code())?.to_639_1())
        .and_then(stop_words::lookup)
        .unwrap_or_else(|| stop_words::get("en"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fillers_follow_the_language_of_the_text() {
        let french =
            of("Le serveur de recette est mis à jour à chaque fusion sur la branche principale.");
        assert!(french.contains(&"les") && !french.contains(&"the"));
        let english = of("The staging server is updated on every merge to the main branch.");
        assert!(english.contains(&"the") && !english.contains(&"les"));
    }

    #[test]
    fn fillers_are_english_when_the_language_is_unclear() {
        assert_eq!(of("CI"), stop_words::get("en"));
    }
}
