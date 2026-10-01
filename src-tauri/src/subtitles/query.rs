//! Turns a video page title into a subtitle search query.

use std::sync::LazyLock;

use regex::Regex;

static BRACKETS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[[^\]]*\]|\([^)]*\)|【[^】]*】").unwrap());
static NOISE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(full\s+movie|full\s+hd|hd|uhd|4k|2160p|1080p|720p|480p|official|with\s+english\s+subtitles|eng(lish)?\s+subs?|subtitles?|полный\s+фильм|фильм|смотреть\s+онлайн)\b",
    )
    .unwrap()
});
/// Pipes, bullets and emoji usually separate the title from a channel name or tags.
static SEPARATOR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[|•\p{So}]").unwrap());
static SPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

pub fn from_title(title: &str) -> String {
    // "Movie (2010) | Channel" -> keep the first non-empty part.
    let head = SEPARATOR_RE
        .split(title)
        .find(|part| part.chars().any(char::is_alphanumeric))
        .unwrap_or(title);
    let s = BRACKETS_RE.replace_all(head, " ");
    let s = NOISE_RE.replace_all(&s, " ");
    let s = SPACE_RE.replace_all(&s, " ");
    let s = s.trim().trim_matches(|c: char| c == '-' || c == ':' || c.is_whitespace());
    if s.is_empty() {
        title.trim().to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_titles() {
        assert_eq!(from_title("Inception (2010) Full Movie HD 1080p | Movies"), "Inception");
        assert_eq!(from_title("Mission: Impossible - Fallout [4K]"), "Mission: Impossible - Fallout");
        assert_eq!(from_title("(2010)"), "(2010)");
        assert_eq!(from_title("Breaking Away (1979) (1080p)🌻 Movies"), "Breaking Away");
        assert_eq!(from_title("🎬 Heat (1995)"), "Heat");
    }
}
