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

/// Returns `(token_count, is_real)`. With the `real-tokens` feature this uses a
/// real BPE tokenizer (o200k_base, the GPT-4o/o-series vocab); otherwise it
/// falls back to [`estimate`] and reports `is_real == false`.
#[cfg(feature = "real-tokens")]
pub fn count(s: &str) -> (usize, bool) {
    use std::sync::OnceLock;
    use tiktoken_rs::{o200k_base, CoreBPE};
    // The o200k_base vocab is embedded in the crate, so this loads offline and
    // is cached for the process lifetime.
    static BPE: OnceLock<CoreBPE> = OnceLock::new();
    let bpe = BPE.get_or_init(|| o200k_base().expect("failed to load o200k_base BPE"));
    (bpe.encode_ordinary(s).len(), true)
}

/// Estimate-only fallback when the `real-tokens` feature is off.
#[cfg(not(feature = "real-tokens"))]
pub fn count(s: &str) -> (usize, bool) {
    (estimate(s), false)
}

#[cfg(all(test, feature = "real-tokens"))]
mod real_token_tests {
    use super::*;

    #[test]
    fn count_is_real_and_nonzero() {
        let sample = "show ip bgp summary\nBGP router identifier 10.0.0.1, local AS number 65001";
        let (n, is_real) = count(sample);
        assert!(is_real, "count() must report a real tokenizer with the feature on");
        assert!(n > 0, "real tokenizer must return a non-zero count");
    }

    #[test]
    fn count_differs_from_estimate() {
        let sample = "show ip bgp summary\nNeighbor V AS MsgRcvd MsgSent TblVer InQ OutQ Up/Down State";
        let (n, _) = count(sample);
        // A real BPE tokenizer will not agree with the crude chars*2/7 heuristic.
        assert_ne!(n, estimate(sample), "real count should differ from the estimate");
    }
}
