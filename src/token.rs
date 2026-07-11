/// Rough token estimate for LLM context accounting.
///
/// Network CLI text (IP addresses, counters, fixed-width tables) tokenizes
/// densely on modern BPE vocabularies — around 3.5 characters per token,
/// noticeably worse than English prose. This is an estimate, not a count;
/// it is labeled as such everywhere it is shown.
pub fn estimate(s: &str) -> usize {
    let chars = s.chars().count();
    (chars * 2).div_ceil(7)
}
