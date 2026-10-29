use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RankingMode {
    #[default]
    Smart,
    Quality,
    Smallest,
    Seeders,
    Original,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RankInput {
    pub id: String,
    pub name: String,
    pub description: String,
    pub addon: String,
    pub original_index: usize,
    pub size_bytes: Option<u64>,
    pub seeders: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReasonKind {
    Positive,
    Negative,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScoreReason {
    pub kind: ReasonKind,
    pub label: String,
    pub points: i32,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParsedStream {
    pub quality_height: Option<u16>,
    pub source: Option<String>,
    pub codec: Option<String>,
    pub hdr: bool,
    pub dolby_vision: bool,
    pub audio: Option<String>,
    pub languages: Vec<String>,
    pub release_group: Option<String>,
    pub suspicious: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RankedStream {
    pub input: RankInput,
    pub parsed: ParsedStream,
    pub score: i32,
    pub reasons: Vec<ScoreReason>,
    pub filtered: bool,
}

pub fn rank_streams(
    inputs: impl IntoIterator<Item = RankInput>,
    mode: RankingMode,
    show_filtered: bool,
) -> Vec<RankedStream> {
    let mut streams = inputs
        .into_iter()
        .map(rank_one)
        .filter(|stream| show_filtered || !stream.filtered)
        .collect::<Vec<_>>();

    match mode {
        RankingMode::Original => streams.sort_by_key(|stream| stream.input.original_index),
        RankingMode::Quality => streams.sort_by(|left, right| {
            right
                .parsed
                .quality_height
                .cmp(&left.parsed.quality_height)
                .then_with(|| left.input.original_index.cmp(&right.input.original_index))
        }),
        RankingMode::Smallest => streams.sort_by(|left, right| {
            let left_size = left
                .input
                .size_bytes
                .or_else(|| parse_size_bytes_from_text(&left.input.description))
                .or_else(|| parse_size_bytes_from_text(&left.input.name));
            let right_size = right
                .input
                .size_bytes
                .or_else(|| parse_size_bytes_from_text(&right.input.description))
                .or_else(|| parse_size_bytes_from_text(&right.input.name));
            left_size
                .unwrap_or(u64::MAX)
                .cmp(&right_size.unwrap_or(u64::MAX))
                .then_with(|| left.input.original_index.cmp(&right.input.original_index))
        }),
        RankingMode::Seeders => streams.sort_by(|left, right| {
            let left_seeds = left
                .input
                .seeders
                .or_else(|| parse_seeders_from_text(&left.input.description))
                .or_else(|| parse_seeders_from_text(&left.input.name));
            let right_seeds = right
                .input
                .seeders
                .or_else(|| parse_seeders_from_text(&right.input.description))
                .or_else(|| parse_seeders_from_text(&right.input.name));
            right_seeds
                .unwrap_or_default()
                .cmp(&left_seeds.unwrap_or_default())
                .then_with(|| left.input.original_index.cmp(&right.input.original_index))
        }),
        RankingMode::Smart => streams.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| compare_quality(right, left))
                .then_with(|| left.input.original_index.cmp(&right.input.original_index))
        }),
    }
    streams
}

fn compare_quality(left: &RankedStream, right: &RankedStream) -> Ordering {
    left.parsed.quality_height.cmp(&right.parsed.quality_height)
}

fn rank_one(input: RankInput) -> RankedStream {
    let parsed = parse_stream(&input);
    let mut score = 0;
    let mut reasons = Vec::new();
    let mut reason = |kind, label: String, points| {
        score += points;
        reasons.push(ScoreReason {
            kind,
            label,
            points,
        });
    };

    if let Some(height) = parsed.quality_height {
        let points = match height {
            2160.. => 50,
            1440..=2159 => 42,
            1080..=1439 => 34,
            720..=1079 => 22,
            480..=719 => 10,
            _ => 2,
        };
        reason(ReasonKind::Positive, format!("{height}p video"), points);
    }
    if let Some(source) = parsed.source.as_deref() {
        let points = match source {
            "BluRay" | "REMUX" => 18,
            "WEB-DL" => 13,
            "WEBRip" => 8,
            "HDTV" => 4,
            "CAM" | "TS" => -35,
            _ => 0,
        };
        reason(
            if points >= 0 {
                ReasonKind::Positive
            } else {
                ReasonKind::Negative
            },
            format!("{source} source"),
            points,
        );
    }
    if let Some(codec) = parsed.codec.as_deref() {
        let points = match codec {
            "AV1" => 10,
            "HEVC" => 8,
            "H.264" => 4,
            _ => 0,
        };
        reason(ReasonKind::Positive, format!("{codec} codec"), points);
    }
    if parsed.dolby_vision {
        reason(ReasonKind::Positive, "Dolby Vision".to_owned(), 8);
    } else if parsed.hdr {
        reason(ReasonKind::Positive, "HDR video".to_owned(), 6);
    }
    if let Some(audio) = parsed.audio.as_deref() {
        let points = if audio.contains("Atmos") || audio.contains("TrueHD") {
            8
        } else if audio.contains("DTS") {
            6
        } else {
            2
        };
        reason(ReasonKind::Positive, format!("{audio} audio"), points);
    }
    let effective_seeders = input
        .seeders
        .or_else(|| parse_seeders_from_text(&input.description))
        .or_else(|| parse_seeders_from_text(&input.name));
    if let Some(seeders) = effective_seeders {
        let points = match seeders {
            500.. => 18,
            100..=499 => 14,
            25..=99 => 9,
            5..=24 => 4,
            1..=4 => 0,
            0 => -18,
        };
        reason(
            if points >= 0 {
                ReasonKind::Positive
            } else {
                ReasonKind::Negative
            },
            format!("{seeders} seeders"),
            points,
        );
    }
    let effective_size = input
        .size_bytes
        .or_else(|| parse_size_bytes_from_text(&input.description))
        .or_else(|| parse_size_bytes_from_text(&input.name));
    if let Some(size) = effective_size {
        let gib = size as f64 / 1_073_741_824.0;
        let points = if gib <= 0.15 {
            -10
        } else if gib <= 8.0 {
            6
        } else if gib >= 80.0 {
            -12
        } else {
            0
        };
        reason(
            if points >= 0 {
                ReasonKind::Positive
            } else {
                ReasonKind::Negative
            },
            format!("{gib:.1} GiB"),
            points,
        );
    }
    if parsed.suspicious {
        reason(
            ReasonKind::Negative,
            "Potential fake, password, or scam result".to_owned(),
            -1_000,
        );
    }

    RankedStream {
        input,
        parsed: parsed.clone(),
        score,
        reasons,
        filtered: parsed.suspicious,
    }
}

pub fn parse_stream(input: &RankInput) -> ParsedStream {
    let combined = format!("{} {}", input.name, input.description);
    let upper = combined.to_ascii_uppercase();
    let quality_height = [4320_u16, 2160, 1440, 1080, 720, 576, 540, 480, 360]
        .into_iter()
        .find(|height| {
            upper.contains(&format!("{height}P"))
                || (*height == 2160 && upper.contains("4K"))
                || (*height == 4320 && upper.contains("8K"))
        });
    let source = [
        ("REMUX", "REMUX"),
        ("BLURAY", "BluRay"),
        ("BLU-RAY", "BluRay"),
        ("WEB-DL", "WEB-DL"),
        ("WEBDL", "WEB-DL"),
        ("WEBRIP", "WEBRip"),
        ("HDTV", "HDTV"),
        ("CAM", "CAM"),
        ("TELESYNC", "TS"),
    ]
    .into_iter()
    .find_map(|(marker, normalized)| upper.contains(marker).then(|| normalized.to_owned()));
    let codec = [
        (&["AV1", "AV01"][..], "AV1"),
        (&["HEVC", "H265", "H.265", "X265"][..], "HEVC"),
        (&["H264", "H.264", "X264", "AVC"][..], "H.264"),
    ]
    .into_iter()
    .find_map(|(markers, normalized)| {
        markers
            .iter()
            .any(|marker| upper.contains(marker))
            .then(|| normalized.to_owned())
    });
    let dolby_vision = ["DOLBY VISION", "DOVI", " DV "]
        .iter()
        .any(|marker| upper.contains(marker));
    let hdr = dolby_vision
        || ["HDR10+", "HDR10", " HDR ", "HLG"]
            .iter()
            .any(|marker| upper.contains(marker));
    let audio = [
        "TRUEHD ATMOS",
        "ATMOS",
        "TRUEHD",
        "DTS-HD",
        "DTS",
        "EAC3",
        "DDP",
        "AAC",
    ]
    .into_iter()
    .find(|marker| upper.contains(marker))
    .map(ToOwned::to_owned);
    let languages = parse_languages(&combined);
    let release_group = combined
        .split_whitespace()
        .rev()
        .find_map(|token| token.strip_prefix('-'))
        .filter(|group| !group.is_empty() && group.len() <= 24)
        .map(ToOwned::to_owned);
    let suspicious = [
        "PASSWORD",
        "PASSW0RD",
        "CLICK HERE",
        "FREE BITCOIN",
        "INSTALL CODEC",
        "EXE INCLUDED",
        "SCAM",
        "FAKE",
    ]
    .iter()
    .any(|marker| upper.contains(marker));

    ParsedStream {
        quality_height,
        source,
        codec,
        hdr,
        dolby_vision,
        audio,
        languages,
        release_group,
        suspicious,
    }
}

/// Display name paired with the uppercased markers that identify it in stream
/// titles and descriptions. Short ISO-style codes are matched as whole words
/// so `ENG` cannot fire inside `ENGINE` or `SPA` inside `SPACE`.
const LANGUAGE_MARKERS: &[(&str, &[&str])] = &[
    ("English", &["ENGLISH", "ENG"]),
    ("Italian", &["ITALIAN", "ITALIANO", "ITA"]),
    ("Russian", &["RUSSIAN", "RUSSKIY", "RUS"]),
    ("German", &["GERMAN", "DEUTSCH", "GER"]),
    ("French", &["FRENCH", "FRANCAIS", "FRA"]),
    ("Spanish", &["SPANISH", "ESPANOL", "CAST", "SPA"]),
    ("Portuguese", &["PORTUGUESE", "PORTUGUES", "POR"]),
    ("Hindi", &["HINDI", "HIN"]),
    ("Arabic", &["ARABIC", "ARA"]),
    ("Japanese", &["JAPANESE", "JPN"]),
    ("Korean", &["KOREAN", "KOR"]),
    ("Chinese", &["CHINESE", "CHS", "CHT", "MANDARIN"]),
    ("Turkish", &["TURKISH", "TUR"]),
    ("Dutch", &["DUTCH", "NLD"]),
    ("Polish", &["POLISH", "POL"]),
    ("Swedish", &["SWEDISH", "SWE"]),
    ("Multi", &["MULTI", "DUAL"]),
];

pub fn parse_languages(text: &str) -> Vec<String> {
    let upper = text.to_ascii_uppercase();
    LANGUAGE_MARKERS
        .iter()
        .filter(|(_, markers)| {
            markers
                .iter()
                .any(|marker| contains_ascii_word(&upper, marker))
        })
        .map(|(language, _)| (*language).to_owned())
        .collect()
}

fn contains_ascii_word(haystack: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(offset) = haystack[from..].find(needle) {
        let start = from + offset;
        let end = start + needle.len();
        let boundary_before = start == 0 || !haystack.as_bytes()[start - 1].is_ascii_alphabetic();
        let boundary_after =
            end == haystack.len() || !haystack.as_bytes()[end].is_ascii_alphabetic();
        if boundary_before && boundary_after {
            return true;
        }
        from = end;
    }
    false
}

pub fn parse_size_bytes_from_text(text: &str) -> Option<u64> {
    if let Some(idx) = text.find('💾') {
        let after = &text[idx + '💾'.len_utf8()..];
        if let Some(bytes) = parse_first_size(after) {
            return Some(bytes);
        }
    }
    parse_first_size(text)
}

fn parse_first_size(text: &str) -> Option<u64> {
    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        let clean_word = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.');
        if let Some(bytes) = parse_size_token(clean_word) {
            return Some(bytes);
        }
        if let Ok(num) = clean_word.parse::<f64>()
            && let Some(unit) = words.get(i + 1)
            && let Some(multiplier) =
                unit_multiplier(unit.trim_matches(|c: char| !c.is_ascii_alphabetic()))
        {
            return Some((num * multiplier) as u64);
        }
    }
    None
}

fn unit_multiplier(unit: &str) -> Option<f64> {
    match unit.to_ascii_uppercase().as_str() {
        "TB" | "TIB" => Some(1_099_511_627_776.0),
        "GB" | "GIB" => Some(1_073_741_824.0),
        "MB" | "MIB" => Some(1_048_576.0),
        "KB" | "KIB" => Some(1_024.0),
        "B" | "BYTES" => Some(1.0),
        _ => None,
    }
}

fn parse_size_token(token: &str) -> Option<u64> {
    let upper = token.to_ascii_uppercase();
    for (unit, mult) in [
        ("TIB", 1_099_511_627_776.0),
        ("TB", 1_099_511_627_776.0),
        ("GIB", 1_073_741_824.0),
        ("GB", 1_073_741_824.0),
        ("MIB", 1_048_576.0),
        ("MB", 1_048_576.0),
        ("KIB", 1_024.0),
        ("KB", 1_024.0),
    ] {
        if let Some(num_part) = upper.strip_suffix(unit)
            && let Ok(num) = num_part.parse::<f64>()
        {
            return Some((num * mult) as u64);
        }
    }
    None
}

pub fn parse_seeders_from_text(text: &str) -> Option<u32> {
    if let Some(idx) = text.find('👤') {
        let after = &text[idx + '👤'.len_utf8()..];
        let num_str: String = after
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(val) = num_str.parse::<u32>() {
            return Some(val);
        }
    }
    let upper = text.to_ascii_uppercase();
    for part in upper.split(['\n', '/', '|', '•', ',', ';']) {
        let tokens: Vec<&str> = part.split_whitespace().collect();
        for (i, token) in tokens.iter().enumerate() {
            let clean = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
            if matches!(clean, "S" | "SEEDS" | "SEEDERS")
                && let Some(next) = tokens.get(i + 1)
                && let Ok(val) = next
                    .trim_matches(|c: char| !c.is_ascii_digit())
                    .parse::<u32>()
            {
                return Some(val);
            }
            if clean.ends_with("SEEDS") || clean.ends_with("SEEDERS") {
                let num_part = clean.trim_end_matches(|c: char| c.is_ascii_alphabetic());
                if let Ok(val) = num_part.parse::<u32>() {
                    return Some(val);
                }
            }
            if let Ok(val) = clean.parse::<u32>()
                && tokens.get(i + 1).is_some_and(|next| {
                    matches!(
                        next.trim_matches(|c: char| !c.is_ascii_alphabetic()),
                        "SEEDS" | "SEEDERS" | "PEERS" | "S"
                    )
                })
            {
                return Some(val);
            }
        }
    }
    None
}

pub fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * 1024 * 1024;

    if bytes >= GIB {
        let whole = bytes / GIB;
        let frac = (bytes % GIB) * 100 / GIB;
        format!("{whole}.{frac:02} GB")
    } else if bytes >= MIB {
        let whole = bytes / MIB;
        let frac = (bytes % MIB) * 10 / MIB;
        format!("{whole}.{frac} MB")
    } else if bytes >= KIB {
        format!("{} KB", bytes / KIB)
    } else {
        format!("{bytes} B")
    }
}
pub fn format_stream_description(
    name: &str,
    description: &str,
    size_bytes: Option<u64>,
    seeders: Option<u32>,
) -> String {
    let seeders = seeders
        .or_else(|| parse_seeders_from_text(description))
        .or_else(|| parse_seeders_from_text(name));
    let size_bytes = size_bytes
        .or_else(|| parse_size_bytes_from_text(description))
        .or_else(|| parse_size_bytes_from_text(name));

    let raw_lines: Vec<&str> = description
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();

    if raw_lines.is_empty() {
        let mut stats = Vec::new();
        if let Some(s) = seeders {
            stats.push(format!("👤 {s}"));
        }
        if let Some(size) = size_bytes {
            stats.push(format!("💾 {}", format_size(size)));
        }
        return stats.join(" ");
    }

    let mut title_lines = Vec::new();
    let mut meta_lines = Vec::new();
    for line in &raw_lines {
        let upper = line.to_ascii_uppercase();
        let is_meta = line.contains('👤')
            || line.contains('💾')
            || line.contains('⚙')
            || line.contains('≡')
            || line.contains('🗓')
            || line.contains('📺')
            || line.contains('⚡')
            || upper.contains("SEEDERS")
            || upper.contains("SEEDS");
        if is_meta {
            meta_lines.push((*line).to_owned());
        } else {
            title_lines.push((*line).to_owned());
        }
    }

    let title = title_lines
        .first()
        .cloned()
        .unwrap_or_else(|| raw_lines[0].to_owned());
    let mut meta_line = meta_lines.join(" ");
    let meta_upper = meta_line.to_ascii_uppercase();

    if let Some(s) = seeders
        && !meta_line.contains('👤')
        && !meta_upper.contains("SEED")
    {
        if !meta_line.is_empty() {
            meta_line.push(' ');
        }
        meta_line.push_str(&format!("👤 {s}"));
    }
    if let Some(size) = size_bytes
        && !meta_line.contains('💾')
        && !meta_upper.contains("GB")
        && !meta_upper.contains("MB")
        && !meta_upper.contains("GIB")
        && !meta_upper.contains("MIB")
    {
        if !meta_line.is_empty() {
            meta_line.push(' ');
        }
        meta_line.push_str(&format!("💾 {}", format_size(size)));
    }

    if meta_line.is_empty() {
        title
    } else {
        format!("{title}\n{meta_line}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(index: usize, name: &str, size: u64, seeders: u32) -> RankInput {
        RankInput {
            id: index.to_string(),
            name: name.to_owned(),
            description: String::new(),
            addon: "Fixture".to_owned(),
            original_index: index,
            size_bytes: Some(size),
            seeders: Some(seeders),
        }
    }

    #[test]
    fn original_mode_exactly_preserves_addon_order() {
        let ranked = rank_streams(
            [
                input(2, "2160p REMUX", 50, 10),
                input(0, "720p WEBRip", 1, 100),
                input(1, "1080p WEB-DL", 2, 50),
            ],
            RankingMode::Original,
            false,
        );
        assert_eq!(
            ranked
                .iter()
                .map(|stream| stream.input.original_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn smart_ranking_is_deterministic_and_uses_original_order_as_final_tie_breaker() {
        let fixture = [
            input(1, "1080p WEB-DL x265", 2_000_000_000, 50),
            input(0, "1080p WEB-DL x265", 2_000_000_000, 50),
        ];
        let first = rank_streams(fixture.clone(), RankingMode::Smart, false);
        let second = rank_streams(fixture, RankingMode::Smart, false);
        assert_eq!(first, second);
        assert_eq!(first[0].input.original_index, 0);
    }

    #[test]
    fn suspicious_streams_are_hidden_but_recoverable() {
        let fake = input(0, "2160p FAKE install codec", 1, 9999);
        assert!(rank_streams([fake.clone()], RankingMode::Smart, false).is_empty());
        let visible = rank_streams([fake], RankingMode::Smart, true);
        assert!(visible[0].filtered);
    }

    #[test]
    fn parses_seeders_and_size_correctly() {
        let desc = "Musafir.Cafe.2026.S01.1080p.NF.WEB-DL.Multi..DD+.5.1.Atmos.x264-KiN\nMusafir Cafe-S01E01-Arrival.1080p.Multi.WEB-DL.DD+.5.1.Atmos.x264-KiN..mkv\n👤 5 💾 1.24 GB ≡ EZTV";
        assert_eq!(parse_seeders_from_text(desc), Some(5));
        assert_eq!(
            parse_size_bytes_from_text(desc),
            Some((1.24 * 1_073_741_824.0) as u64)
        );

        let formatted = format_stream_description("Torrentio 1080p", desc, None, None);
        assert!(formatted.contains("👤 5"));
        assert!(formatted.contains("💾 1.24 GB"));
        assert_eq!(formatted.lines().count(), 2);
    }

    #[test]
    fn ensures_seeds_and_size_when_only_in_behavior_hints() {
        let formatted = format_stream_description(
            "HTTP Stream",
            "Big Buck Bunny 1080p",
            Some(1_331_691_520),
            Some(12),
        );
        assert!(formatted.contains("👤 12"));
        assert!(formatted.contains("💾 1.24 GB"));
        assert_eq!(formatted.lines().count(), 2);
    }

    #[test]
    fn detects_languages_as_whole_words_only() {
        assert_eq!(
            parse_languages("Movie.2024.1080p.[ITA] Rus.Eng x264"),
            vec!["English", "Italian", "Russian"]
        );
        assert!(parse_languages("ENGINEERING SPACE TURBO SWEET").is_empty());
        assert!(parse_languages("1080p WEB-DL x264 MULTI").contains(&"Multi".to_owned()));
    }
}
