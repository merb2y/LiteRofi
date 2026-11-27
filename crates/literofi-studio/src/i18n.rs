#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Tr,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Tr => "tr",
            Lang::En => "en",
        }
    }

    pub fn from_code(s: &str) -> Self {
        if s.eq_ignore_ascii_case("en") {
            Lang::En
        } else {
            Lang::Tr
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::Tr => "TR",
            Lang::En => "EN",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Lang::Tr => Lang::En,
            Lang::En => Lang::Tr,
        }
    }
}

pub fn pick(lang: Lang, tr: &'static str, en: &'static str) -> &'static str {
    match lang {
        Lang::Tr => tr,
        Lang::En => en,
    }
}

pub fn category_label(lang: Lang, cat: &str) -> &str {
    match cat {
        "All" => pick(lang, "Tümü", "All"),
        "Favorites" => pick(lang, "Favoriler", "Favorites"),
        "Classic" | "Klasik" => pick(lang, "Klasik", "Classic"),
        other => other,
    }
}
